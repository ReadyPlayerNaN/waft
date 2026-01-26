//! Tokio-glib runtime bridge.
//!
//! When awaiting tokio-dependent futures (reqwest, tokio::process, zbus, etc.)
//! from inside `glib::spawn_future_local`, glib's poll loop does not integrate
//! with tokio's I/O driver. This causes glib to busy-poll with zero-timeout
//! `ppoll` calls, resulting in 100% CPU usage.
//!
//! [`spawn_on_tokio`] bridges this gap by spawning the work on the tokio runtime
//! and delivering the result through a `flume` channel, which is
//! executor-agnostic and safe to await from glib.

use std::future::Future;

/// Run an async task on the tokio runtime and return its result.
///
/// Use this when you need to `.await` a tokio-dependent future from inside a
/// glib async context (e.g. `glib::spawn_future_local`).
pub async fn spawn_on_tokio<F, T>(future: F) -> T
where
    F: Future<Output = T> + Send + 'static,
    T: Send + 'static,
{
    let (tx, rx) = flume::bounded(1);
    tokio::spawn(async move {
        let result = future.await;
        let _ = tx.send(result);
    });
    rx.recv_async()
        .await
        .expect("tokio task was cancelled or panicked")
}
