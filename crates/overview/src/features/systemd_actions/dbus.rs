//! D-Bus integration for systemd login1 Manager.
//!
//! Provides access to system power and session management actions via D-Bus.
//!
//! ## D-Bus Interfaces
//!
//! - `org.freedesktop.login1.Manager` - System-wide power operations (reboot, shutdown, suspend)
//! - `org.freedesktop.login1.Session` - Session-specific operations (lock, logout)
//!
//! ## PolicyKit Authorization
//!
//! Power operations (reboot, shutdown, suspend) require PolicyKit authorization.
//! The `interactive` flag allows PolicyKit to prompt for credentials.

use anyhow::{Context, Result};
use log::{info, warn};
use std::env;
use std::sync::Arc;

use crate::dbus::DbusHandle;

const LOGIN1_SERVICE: &str = "org.freedesktop.login1";
const MANAGER_PATH: &str = "/org/freedesktop/login1";
const MANAGER_INTERFACE: &str = "org.freedesktop.login1.Manager";
const SESSION_INTERFACE: &str = "org.freedesktop.login1.Session";

/// System action variants.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SystemAction {
    /// Lock the current session.
    LockSession,
    /// Terminate (logout) the current session.
    Terminate,
    /// Reboot the system.
    Reboot { interactive: bool },
    /// Power off (shutdown) the system.
    PowerOff { interactive: bool },
    /// Suspend the system.
    Suspend { interactive: bool },
}

/// D-Bus client for systemd login1 operations.
pub struct SystemdDbusClient {
    dbus: Arc<DbusHandle>,
    session_path: String,
}

impl SystemdDbusClient {
    /// Create a new systemd D-Bus client.
    ///
    /// Returns `None` if the login1 service is unavailable (graceful degradation).
    pub async fn new(dbus: Arc<DbusHandle>) -> Option<Self> {
        let session_path = match Self::get_session_path() {
            Ok(path) => path,
            Err(e) => {
                warn!("[systemd-actions] Failed to resolve session path: {}", e);
                warn!("[systemd-actions] Continuing without session actions support");
                return None;
            }
        };

        info!("[systemd-actions] Using session path: {}", session_path);

        Some(Self { dbus, session_path })
    }

    /// Get the current session's D-Bus object path.
    ///
    /// Checks `XDG_SESSION_ID` environment variable and falls back to `/session/auto`.
    fn get_session_path() -> Result<String> {
        // Try XDG_SESSION_ID first (most reliable)
        if let Ok(session_id) = env::var("XDG_SESSION_ID") {
            return Ok(format!("/org/freedesktop/login1/session/{}", session_id));
        }

        // Fallback: use "auto" which logind resolves to the caller's session
        Ok("/org/freedesktop/login1/session/auto".to_string())
    }

    /// Execute a system action via D-Bus.
    pub async fn execute_action(&self, action: SystemAction) -> Result<()> {
        match action {
            SystemAction::LockSession => self.lock_session().await,
            SystemAction::Terminate => self.terminate_session().await,
            SystemAction::Reboot { interactive } => self.reboot(interactive).await,
            SystemAction::PowerOff { interactive } => self.power_off(interactive).await,
            SystemAction::Suspend { interactive } => self.suspend(interactive).await,
        }
    }

    /// Lock the current session.
    async fn lock_session(&self) -> Result<()> {
        self.dbus
            .connection()
            .call_method(
                Some(LOGIN1_SERVICE),
                self.session_path.as_str(),
                Some(SESSION_INTERFACE),
                "Lock",
                &(),
            )
            .await
            .context("Failed to lock session")?;

        info!("[systemd-actions] Session locked");
        Ok(())
    }

    /// Terminate (logout) the current session.
    async fn terminate_session(&self) -> Result<()> {
        self.dbus
            .connection()
            .call_method(
                Some(LOGIN1_SERVICE),
                self.session_path.as_str(),
                Some(SESSION_INTERFACE),
                "Terminate",
                &(),
            )
            .await
            .context("Failed to terminate session")?;

        info!("[systemd-actions] Session terminated");
        Ok(())
    }

    /// Reboot the system.
    ///
    /// If `interactive` is true, PolicyKit may prompt for authorization.
    async fn reboot(&self, interactive: bool) -> Result<()> {
        self.dbus
            .connection()
            .call_method(
                Some(LOGIN1_SERVICE),
                MANAGER_PATH,
                Some(MANAGER_INTERFACE),
                "Reboot",
                &(interactive,),
            )
            .await
            .context("Failed to reboot system")?;

        info!(
            "[systemd-actions] Reboot initiated (interactive: {})",
            interactive
        );
        Ok(())
    }

    /// Power off (shutdown) the system.
    ///
    /// If `interactive` is true, PolicyKit may prompt for authorization.
    async fn power_off(&self, interactive: bool) -> Result<()> {
        self.dbus
            .connection()
            .call_method(
                Some(LOGIN1_SERVICE),
                MANAGER_PATH,
                Some(MANAGER_INTERFACE),
                "PowerOff",
                &(interactive,),
            )
            .await
            .context("Failed to power off system")?;

        info!(
            "[systemd-actions] Power off initiated (interactive: {})",
            interactive
        );
        Ok(())
    }

    /// Suspend the system.
    ///
    /// If `interactive` is true, PolicyKit may prompt for authorization.
    async fn suspend(&self, interactive: bool) -> Result<()> {
        self.dbus
            .connection()
            .call_method(
                Some(LOGIN1_SERVICE),
                MANAGER_PATH,
                Some(MANAGER_INTERFACE),
                "Suspend",
                &(interactive,),
            )
            .await
            .context("Failed to suspend system")?;

        info!(
            "[systemd-actions] Suspend initiated (interactive: {})",
            interactive
        );
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_session_path_from_env() {
        // Set XDG_SESSION_ID and verify path construction
        unsafe {
            env::set_var("XDG_SESSION_ID", "42");
        }
        let path = SystemdDbusClient::get_session_path().unwrap();
        assert_eq!(path, "/org/freedesktop/login1/session/42");
        unsafe {
            env::remove_var("XDG_SESSION_ID");
        }
    }

    #[test]
    fn test_session_path_fallback() {
        // Remove XDG_SESSION_ID and verify fallback
        unsafe {
            env::remove_var("XDG_SESSION_ID");
        }
        let path = SystemdDbusClient::get_session_path().unwrap();
        assert_eq!(path, "/org/freedesktop/login1/session/auto");
    }
}
