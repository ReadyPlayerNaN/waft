//! Systemd actions daemon -- system power and session management.
//!
//! Provides a single session entity with the current user's name and display.
//! Handles power and session actions via D-Bus calls to systemd-logind.
//!
//! Actions:
//! - `lock` - Lock the current session
//! - `logout` - Terminate the current session
//! - `reboot` - Reboot the system
//! - `shutdown` - Power off the system
//! - `suspend` - Suspend the system
//!
//! Configuration (in ~/.config/waft/config.toml):
//! ```toml
//! [[plugins]]
//! id = "systemd-actions"
//! ```

use std::sync::OnceLock;

use anyhow::{Context, Result};
use waft_i18n::I18n;
use waft_plugin::*;
use zbus::Connection;

static I18N: OnceLock<I18n> = OnceLock::new();

fn i18n() -> &'static I18n {
    I18N.get_or_init(|| {
        I18n::new(&[
            ("en-US", include_str!("../locales/en-US/systemd-actions.ftl")),
            ("cs-CZ", include_str!("../locales/cs-CZ/systemd-actions.ftl")),
        ])
    })
}

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

/// Get the current user's login name from the environment.
fn get_user_name() -> Option<String> {
    std::env::var("USER")
        .or_else(|_| std::env::var("LOGNAME"))
        .ok()
}

/// Get the current display from the environment.
fn get_screen_name() -> Option<String> {
    std::env::var("WAYLAND_DISPLAY")
        .or_else(|_| std::env::var("DISPLAY"))
        .ok()
}

/// Systemd actions plugin.
///
/// Stateless: the session entity is fixed; actions dispatch D-Bus calls.
struct SystemdActionsPlugin {
    conn: Connection,
    session_path: String,
    user_name: Option<String>,
    screen_name: Option<String>,
}

impl SystemdActionsPlugin {
    async fn new() -> Result<Self> {
        let conn = Connection::system()
            .await
            .context("Failed to connect to system D-Bus")?;

        let session_path = get_session_path();
        log::info!("Using session path: {}", session_path);

        Ok(Self {
            conn,
            session_path,
            user_name: get_user_name(),
            screen_name: get_screen_name(),
        })
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

        log::info!("Manager.{}(interactive={}) executed", method, interactive);
        Ok(())
    }
}

#[async_trait::async_trait]
impl Plugin for SystemdActionsPlugin {
    fn get_entities(&self) -> Vec<Entity> {
        let session = entity::session::Session {
            user_name: self.user_name.clone(),
            screen_name: self.screen_name.clone(),
        };
        vec![Entity::new(
            Urn::new(
                "systemd-actions",
                entity::session::SESSION_ENTITY_TYPE,
                "default",
            ),
            entity::session::SESSION_ENTITY_TYPE,
            &session,
        )]
    }

    async fn handle_action(
        &self,
        _urn: Urn,
        action: String,
        _params: serde_json::Value,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        match action.as_str() {
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

fn main() -> Result<()> {
    if waft_plugin::manifest::handle_provides_i18n(
        &[entity::session::SESSION_ENTITY_TYPE],
        i18n(),
        "plugin-name",
        "plugin-description",
    ) {
        return Ok(());
    }

    waft_plugin::init_plugin_logger("info");

    log::info!("Starting systemd-actions plugin...");

    let rt = tokio::runtime::Runtime::new().context("failed to create tokio runtime")?;
    rt.block_on(async {
        let plugin = SystemdActionsPlugin::new().await?;

        // Stateless plugin: no background tasks, no notifier needed
        let (runtime, _notifier) = PluginRuntime::new("systemd-actions", plugin);

        runtime.run().await?;
        Ok(())
    })
}
