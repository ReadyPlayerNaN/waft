mod action_tracker;
mod connection;
mod daemon;
mod registry;

use std::path::PathBuf;

use daemon::WaftDaemon;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let socket_path = daemon_socket_path()?;

    eprintln!(
        "[waft] starting daemon at {}",
        socket_path.display()
    );

    let daemon = WaftDaemon::new(socket_path)?;
    daemon.run().await?;

    Ok(())
}

fn daemon_socket_path() -> Result<PathBuf, Box<dyn std::error::Error>> {
    let runtime_dir = std::env::var("XDG_RUNTIME_DIR")
        .map_err(|_| "XDG_RUNTIME_DIR not set")?;

    let mut path = PathBuf::from(runtime_dir);
    path.push("waft");
    std::fs::create_dir_all(&path)?;
    path.push("daemon.sock");

    // Remove stale socket from previous run
    if path.exists() {
        std::fs::remove_file(&path)?;
    }

    Ok(path)
}
