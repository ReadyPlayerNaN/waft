mod action_tracker;
mod claim_tracker;
mod cli;
mod commands_command;
mod connection;
mod crash_tracker;
mod daemon;
mod plugin_discovery;
mod plugin_spawner;
mod protocol_command;
mod query_command;
mod registry;
mod socket_io;

use std::path::PathBuf;

use clap::Parser;
use cli::{Cli, Command, PluginCommand};
use daemon::WaftDaemon;
use log::info;

/// Well-known D-Bus name for the waft daemon.
const DBUS_NAME: &str = "org.waft.Daemon";

fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let cli = Cli::parse();

    match cli.command {
        None | Some(Command::Daemon) => {
            env_logger::Builder::from_env(
                env_logger::Env::default().default_filter_or("info"),
            )
            .init();
            run_daemon()?;
        }
        Some(Command::Plugin { command }) => match command {
            PluginCommand::Ls => {
                plugin_discovery::print_plugin_list(cli.json);
            }
            PluginCommand::Describe { name } => {
                plugin_discovery::print_plugin_description(&name, cli.json);
            }
        },
        Some(Command::Commands { filter, run }) => {
            commands_command::run(cli.json, filter.as_deref(), run);
        }
        Some(Command::Protocol { entity_type, domain, verbose }) => {
            protocol_command::run(
                cli.json,
                verbose,
                entity_type.as_deref(),
                domain.as_deref(),
            );
        }
        Some(Command::Query { entity_type, start, timeout_ms }) => {
            query_command::run(cli.json, entity_type.as_deref(), start, timeout_ms);
        }
    }

    Ok(())
}

fn run_daemon() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let rt = tokio::runtime::Runtime::new()?;
    rt.block_on(async {
        // Register on the session bus so clients can discover us and
        // D-Bus activation can auto-start the daemon. A failure here means
        // another instance already owns the name — refuse to start rather
        // than running headless and racing the existing daemon.
        let dbus_conn = register_dbus_name().await?;
        info!("registered as {DBUS_NAME} on session bus");

        let socket_path = daemon_socket_path()?;

        info!("starting daemon at {}", socket_path.display());

        let daemon = WaftDaemon::new(&socket_path)?;
        daemon.run().await?;

        // Keep the D-Bus connection alive for the daemon's lifetime
        drop(dbus_conn);

        Ok(())
    })
}

/// Detect whether a daemon is already listening on the given socket path.
///
/// Returns:
/// - `Ok(true)`  — file is present and a process accepts connections (live daemon).
/// - `Ok(false)` — file is missing, or present but refuses connections (orphan).
/// - `Err(_)`    — the probe itself failed (permission denied, etc.).
fn probe_existing_listener(path: &std::path::Path) -> std::io::Result<bool> {
    use std::io::ErrorKind;

    match std::os::unix::net::UnixStream::connect(path) {
        Ok(_) => Ok(true),
        Err(e) => match e.kind() {
            ErrorKind::NotFound | ErrorKind::ConnectionRefused => Ok(false),
            _ => Err(e),
        },
    }
}

/// Request the well-known D-Bus name on the session bus.
///
/// Returns the connection handle (must be kept alive). Fails if another
/// instance already owns the name.
async fn register_dbus_name() -> Result<zbus::Connection, Box<dyn std::error::Error + Send + Sync>> {
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

fn daemon_socket_path() -> Result<PathBuf, Box<dyn std::error::Error + Send + Sync>> {
    let runtime_dir = std::env::var("XDG_RUNTIME_DIR").map_err(|_| "XDG_RUNTIME_DIR not set")?;

    let mut path = PathBuf::from(runtime_dir);
    path.push("waft");
    std::fs::create_dir_all(&path)?;
    path.push("daemon.sock");

    // Probe before unlinking: if a real daemon is listening, refuse to clobber
    // its socket. Only remove the file when it's an orphan from a previous run.
    if path.exists() {
        if probe_existing_listener(&path)? {
            return Err(format!(
                "another waft daemon is already listening on {}",
                path.display()
            )
            .into());
        }
        std::fs::remove_file(&path)?;
    }

    Ok(path)
}
