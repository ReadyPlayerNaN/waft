//! Tokio-glib runtime bridge for cdylib plugins.
//!
//! When awaiting tokio-dependent futures (reqwest, tokio::process, zbus, etc.)
//! from inside `glib::spawn_future_local`, glib's poll loop does not integrate
//! with tokio's I/O driver. This causes glib to busy-poll with zero-timeout
//! `ppoll` calls, resulting in 100% CPU usage.
//!
//! [`spawn_on_tokio`] bridges this gap by spawning the work on a persistent
//! plugin-local tokio runtime and delivering the result through a `flume`
//! channel, which is executor-agnostic and safe to await from glib.
//!
//! **Why a plugin-local runtime instead of the host's handle?**
//! In cdylib plugins, each .so gets its own copy of tokio with separate
//! thread-local storage. Using the host's Handle to spawn tasks puts them on
//! host worker threads where the plugin's tokio has no context. When zbus
//! proxies are dropped on those threads, they call the plugin's
//! `tokio::spawn` which panics. A plugin-local current-thread runtime avoids
//! this: all tasks (including internally-spawned ones from zbus
//! PropertiesCache) run on the same dedicated thread with full plugin-local
//! tokio context.

use std::future::Future;
use std::sync::{Arc, OnceLock};

use log::error;

/// Global tokio runtime handle for async operations.
/// Must be initialized before calling any async functions in this module.
static TOKIO_HANDLE: OnceLock<Arc<tokio::runtime::Handle>> = OnceLock::new();

/// Plugin-local tokio runtime handle. Created once, lives for plugin lifetime.
pub static PLUGIN_RT_HANDLE: OnceLock<tokio::runtime::Handle> = OnceLock::new();

/// Initialize the host's tokio runtime handle for this module.
/// Must be called before any other functions in this module.
pub fn init_tokio_handle(handle: tokio::runtime::Handle) {
    TOKIO_HANDLE.get_or_init(|| Arc::new(handle));

    // Also create the plugin-local runtime on a dedicated thread.
    PLUGIN_RT_HANDLE.get_or_init(|| {
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::Builder::new()
            .name("notifications-plugin-rt".to_string())
            .spawn(move || {
                let rt = tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                    .expect("Failed to create plugin-local tokio runtime");
                tx.send(rt.handle().clone()).expect("Failed to send handle");
                // Block forever to keep the runtime alive. The runtime
                // processes spawned tasks while blocked here.
                rt.block_on(std::future::pending::<()>());
            })
            .expect("Failed to spawn plugin runtime thread");
        rx.recv().expect("Failed to receive plugin runtime handle")
    });
}

/// Run an async task on the plugin-local tokio runtime.
///
/// Spawns the future on the plugin's persistent current-thread runtime
/// and delivers the result via flume. This ensures all internally-spawned
/// tokio tasks (e.g. from zbus PropertiesCache) run on the same thread
/// with full runtime context, avoiding the cdylib tokio TLS issue.
pub async fn spawn_on_tokio<F, T>(future: F) -> T
where
    F: Future<Output = T> + Send + 'static,
    T: Send + 'static,
{
    let (tx, rx) = flume::bounded(1);
    let handle = PLUGIN_RT_HANDLE
        .get()
        .expect("plugin runtime not initialized - call init_tokio_handle first");

    handle.spawn(async move {
        let result = future.await;
        let _ = tx.send(result);
    });

    match rx.recv_async().await {
        Ok(val) => val,
        Err(e) => {
            error!("[runtime] tokio task was cancelled or panicked: {e}");
            panic!("tokio task was cancelled or panicked: {e}");
        }
    }
}

/// Get the plugin-local tokio runtime handle.
/// This is used for spawning tasks directly on the plugin's runtime.
pub fn get_handle() -> &'static tokio::runtime::Handle {
    PLUGIN_RT_HANDLE.get()
        .expect("plugin runtime not initialized - call init_tokio_handle first")
}
