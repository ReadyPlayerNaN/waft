mod action_tracker;
mod connection;
mod crash_tracker;
mod daemon;
mod plugin_discovery;
mod plugin_spawner;
mod registry;

use std::path::PathBuf;

use daemon::WaftDaemon;

/// Well-known D-Bus name for the waft daemon.
const DBUS_NAME: &str = "org.waft.Daemon";

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().collect();

    if args.len() >= 3 && args[1] == "plugin" && args[2] == "ls" {
        plugin_discovery::print_plugin_list();
        return Ok(());
    }

    let rt = tokio::runtime::Runtime::new()?;
    rt.block_on(async {
        // Register on the session bus so clients can discover us and
        // D-Bus activation can auto-start the daemon.
        let dbus_conn = match register_dbus_name().await {
            Ok(conn) => {
                eprintln!("[waft] registered as {DBUS_NAME} on session bus");
                Some(conn)
            }
            Err(e) => {
                eprintln!("[waft] failed to register on D-Bus: {e}");
                eprintln!("[waft] continuing without D-Bus name (clients must connect directly)");
                None
            }
        };

        let socket_path = daemon_socket_path()?;

        eprintln!(
            "[waft] starting daemon at {}",
            socket_path.display()
        );

        let daemon = WaftDaemon::new(socket_path)?;
        daemon.run().await?;

        // Keep the D-Bus connection alive for the daemon's lifetime
        drop(dbus_conn);

        Ok(())
    })
}

/// Request the well-known D-Bus name on the session bus.
///
/// Returns the connection handle (must be kept alive). Fails if another
/// instance already owns the name.
async fn register_dbus_name() -> Result<zbus::Connection, Box<dyn std::error::Error>> {
    let conn = zbus::Connection::session().await?;

    let well_known_name = zbus::names::WellKnownName::try_from(DBUS_NAME)?;

    // Request the name with DoNotQueue — fail immediately if already taken
    let reply = conn
        .request_name_with_flags(
            well_known_name,
            zbus::fdo::RequestNameFlags::DoNotQueue.into(),
        )
        .await?;

    match reply {
        zbus::fdo::RequestNameReply::PrimaryOwner => Ok(conn),
        zbus::fdo::RequestNameReply::AlreadyOwner => Ok(conn),
        other => Err(format!(
            "could not acquire D-Bus name {DBUS_NAME}: {other:?} (another instance running?)"
        )
        .into()),
    }
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
