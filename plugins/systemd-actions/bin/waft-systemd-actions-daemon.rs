//! Systemd actions daemon - system power and session management.
//!
//! This daemon provides quick access to system power and session management
//! actions via D-Bus calls to org.freedesktop.login1 (systemd-logind).
//!
//! Widgets:
//! - Session actions: Lock Session, Logout
//! - Power actions: Reboot, Shutdown, Suspend
//!
//! This plugin is stateless - widget definitions are fixed and only actions
//! trigger D-Bus calls.

use anyhow::{Context, Result};
use waft_plugin_sdk::*;
use zbus::Connection;

const LOGIN1_DESTINATION: &str = "org.freedesktop.login1";
const MANAGER_PATH: &str = "/org/freedesktop/login1";
const MANAGER_INTERFACE: &str = "org.freedesktop.login1.Manager";
const SESSION_INTERFACE: &str = "org.freedesktop.login1.Session";

/// Resolve the current session's D-Bus object path.
///
/// Checks `XDG_SESSION_ID` environment variable and falls back to `/session/auto`.
fn get_session_path() -> String {
    if let Ok(session_id) = std::env::var("XDG_SESSION_ID") {
        format!("/org/freedesktop/login1/session/{}", session_id)
    } else {
        "/org/freedesktop/login1/session/auto".to_string()
    }
}

/// Systemd actions daemon.
///
/// Stateless: widget structure is fixed, actions dispatch D-Bus calls.
struct SystemdActionsDaemon {
    conn: Connection,
    session_path: String,
}

impl SystemdActionsDaemon {
    async fn new() -> Result<Self> {
        let conn = Connection::system()
            .await
            .context("Failed to connect to system D-Bus")?;

        let session_path = get_session_path();
        log::info!("Using session path: {}", session_path);

        Ok(Self { conn, session_path })
    }

    fn build_session_widget(&self) -> Widget {
        ColBuilder::new()
            .spacing(4)
            .child(
                MenuRowBuilder::new("Lock Session")
                    .icon("system-lock-screen-symbolic")
                    .on_click("lock")
                    .build(),
            )
            .child(
                MenuRowBuilder::new("Logout")
                    .icon("system-log-out-symbolic")
                    .on_click("logout")
                    .build(),
            )
            .build()
    }

    fn build_power_widget(&self) -> Widget {
        ColBuilder::new()
            .spacing(4)
            .child(
                MenuRowBuilder::new("Reboot")
                    .icon("system-reboot-symbolic")
                    .on_click("reboot")
                    .build(),
            )
            .child(
                MenuRowBuilder::new("Shutdown")
                    .icon("system-shutdown-symbolic")
                    .on_click("shutdown")
                    .build(),
            )
            .child(
                MenuRowBuilder::new("Suspend")
                    .icon("media-playback-pause-symbolic")
                    .on_click("suspend")
                    .build(),
            )
            .build()
    }

    /// Call a method on the session interface (no arguments).
    async fn call_session_method(&self, method: &str) -> Result<()> {
        let proxy = zbus::Proxy::new(
            &self.conn,
            LOGIN1_DESTINATION,
            self.session_path.as_str(),
            SESSION_INTERFACE,
        )
        .await
        .context("Failed to create session proxy")?;

        let _: () = proxy
            .call(method, &())
            .await
            .with_context(|| format!("Failed to call Session.{}", method))?;

        log::info!("Session.{}() executed", method);
        Ok(())
    }

    /// Call a method on the manager interface with an `interactive: bool` argument.
    async fn call_manager_method(&self, method: &str, interactive: bool) -> Result<()> {
        let proxy = zbus::Proxy::new(
            &self.conn,
            LOGIN1_DESTINATION,
            MANAGER_PATH,
            MANAGER_INTERFACE,
        )
        .await
        .context("Failed to create manager proxy")?;

        let _: () = proxy
            .call(method, &(interactive,))
            .await
            .with_context(|| format!("Failed to call Manager.{}", method))?;

        log::info!(
            "Manager.{}(interactive={}) executed",
            method,
            interactive
        );
        Ok(())
    }
}

#[async_trait::async_trait]
impl PluginDaemon for SystemdActionsDaemon {
    fn get_widgets(&self) -> Vec<NamedWidget> {
        vec![
            NamedWidget {
                id: "systemd-actions:session".to_string(),
                weight: 20,
                widget: self.build_session_widget(),
            },
            NamedWidget {
                id: "systemd-actions:power".to_string(),
                weight: 21,
                widget: self.build_power_widget(),
            },
        ]
    }

    async fn handle_action(
        &self,
        _widget_id: String,
        action: Action,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        match action.id.as_str() {
            "lock" => self.call_session_method("Lock").await?,
            "logout" => self.call_session_method("Terminate").await?,
            "reboot" => self.call_manager_method("Reboot", true).await?,
            "shutdown" => self.call_manager_method("PowerOff", true).await?,
            "suspend" => self.call_manager_method("Suspend", true).await?,
            other => log::warn!("Unknown action: {}", other),
        }
        Ok(())
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    waft_plugin_sdk::init_daemon_logger("info");

    log::info!("Starting systemd-actions daemon...");

    let daemon = SystemdActionsDaemon::new().await?;

    // Stateless plugin: no background tasks, no notifier needed
    let (server, _notifier) = PluginServer::new("systemd-actions-daemon", daemon);

    server.run().await?;

    Ok(())
}
