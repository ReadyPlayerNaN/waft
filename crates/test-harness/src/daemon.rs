use std::path::PathBuf;

use tempfile::TempDir;
use tokio::task::JoinHandle;
use waft::WaftDaemon;

/// A test daemon instance that runs on a temporary Unix socket.
///
/// Sets `WAFT_DAEMON_DIR` to an empty directory so no plugins are discovered.
/// The daemon is spawned as a tokio task and can be shut down by aborting it.
pub struct TestDaemon {
    pub socket_path: PathBuf,
    task: JoinHandle<()>,
    // Keep TempDir alive so the directory (and socket) are not removed.
    _socket_dir: TempDir,
    _plugin_dir: TempDir,
}

impl TestDaemon {
    /// Start a new test daemon on a temporary Unix socket.
    ///
    /// The daemon discovers no plugins (empty `WAFT_DAEMON_DIR`).
    /// The socket is ready for connections immediately after this returns.
    pub async fn start() -> Self {
        let socket_dir = TempDir::new().expect("failed to create temp dir for socket");
        let socket_path = socket_dir.path().join("daemon.sock");

        // Point WAFT_DAEMON_DIR to an empty temp directory so no plugins are discovered.
        let plugin_dir = TempDir::new().expect("failed to create temp dir for plugins");
        // SAFETY: Tests run serialized via #[serial] so no concurrent env var mutation.
        unsafe {
            std::env::set_var("WAFT_DAEMON_DIR", plugin_dir.path());
        }

        let daemon =
            WaftDaemon::new(socket_path.clone()).expect("failed to create test WaftDaemon");

        // Spawn on a LocalSet because WaftDaemon::run() returns Box<dyn Error>
        // which is not Send. We use spawn_blocking + a dedicated runtime instead.
        let task = tokio::spawn(async {
            // WaftDaemon::run() returns Result<(), Box<dyn Error>> where the error
            // is not necessarily Send. We run it on the current runtime in a way
            // that handles the non-Send error locally.
            let result = daemon.run().await;
            if let Err(e) = result {
                log::warn!("[test-daemon] daemon exited with error: {e}");
            }
        });

        TestDaemon {
            socket_path,
            task,
            _socket_dir: socket_dir,
            _plugin_dir: plugin_dir,
        }
    }

    /// Shut down the daemon by aborting its task.
    pub async fn shutdown(self) {
        self.task.abort();
        let _ = self.task.await;
    }
}
