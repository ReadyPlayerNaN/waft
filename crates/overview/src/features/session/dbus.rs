//! Logind session D-Bus integration.
//!
//! Connects to org.freedesktop.login1.Session to receive Lock/Unlock signals.

use anyhow::{Context, Result};
use log::{debug, info, warn};
use std::env;
use std::sync::Arc;
use tokio::sync::broadcast;
use zbus::Connection;

/// Session state change events.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionEvent {
    Lock,
    Unlock,
}

/// Monitor for logind session Lock/Unlock signals.
pub struct SessionMonitor {
    event_tx: broadcast::Sender<SessionEvent>,
}

impl SessionMonitor {
    /// Create a new session monitor and start listening for signals.
    ///
    /// Returns None if logind is unavailable (graceful degradation).
    pub async fn new() -> Option<Self> {
        let (event_tx, _) = broadcast::channel(16);
        let monitor = Self { event_tx };

        if let Err(e) = monitor.start_listener().await {
            warn!("[session] Failed to start session monitor: {e}");
            warn!("[session] Continuing without session lock detection");
            return None;
        }

        Some(monitor)
    }

    /// Subscribe to session events.
    pub fn subscribe(&self) -> broadcast::Receiver<SessionEvent> {
        self.event_tx.subscribe()
    }

    /// Get the current session's D-Bus object path.
    fn get_session_path() -> Result<String> {
        // Try XDG_SESSION_ID first (most reliable)
        if let Ok(session_id) = env::var("XDG_SESSION_ID") {
            return Ok(format!("/org/freedesktop/login1/session/{}", session_id));
        }

        // Fallback: use "auto" which logind resolves to the caller's session
        Ok("/org/freedesktop/login1/session/auto".to_string())
    }

    async fn start_listener(&self) -> Result<()> {
        let conn = Connection::system()
            .await
            .context("Failed to connect to system bus")?;

        let session_path = Self::get_session_path()?;
        info!("[session] Monitoring session at {session_path}");

        let conn = Arc::new(conn);
        let event_tx = self.event_tx.clone();

        // Subscribe to Lock signal
        let lock_rule = format!(
            "type='signal',interface='org.freedesktop.login1.Session',member='Lock',path='{}'",
            session_path
        );

        let unlock_rule = format!(
            "type='signal',interface='org.freedesktop.login1.Session',member='Unlock',path='{}'",
            session_path
        );

        // Start listener for Lock signals
        Self::listen_signal(
            conn.clone(),
            &lock_rule,
            SessionEvent::Lock,
            event_tx.clone(),
        )
        .await?;

        // Start listener for Unlock signals
        Self::listen_signal(conn, &unlock_rule, SessionEvent::Unlock, event_tx).await?;

        Ok(())
    }

    async fn listen_signal(
        conn: Arc<Connection>,
        match_rule: &str,
        event: SessionEvent,
        event_tx: broadcast::Sender<SessionEvent>,
    ) -> Result<()> {
        use futures_util::StreamExt;

        let rule: zbus::MatchRule<'static> = zbus::MatchRule::try_from(match_rule)
            .with_context(|| format!("Invalid match rule: {match_rule}"))?
            .to_owned();

        // Try to add match rule to bus (best effort)
        if let Ok(dbus) = zbus::fdo::DBusProxy::new(&conn).await {
            let _ = dbus.add_match_rule(rule.clone()).await;
        }

        let rule_str = match_rule.to_string();

        tokio::spawn(async move {
            let mut stream = zbus::MessageStream::from(&*conn);

            while let Some(next) = stream.next().await {
                let msg = match next {
                    Ok(m) => m,
                    Err(e) => {
                        debug!("[session] Signal stream error: {e}");
                        continue;
                    }
                };

                // Only process signal messages (type=4)
                let msg_type = msg.header().primary().msg_type();
                if msg_type as u8 != 4 {
                    continue;
                }

                let h = msg.header();

                // Check interface and member match
                let iface_ok = h
                    .interface()
                    .map(|i| i.as_str() == "org.freedesktop.login1.Session")
                    .unwrap_or(false);

                let member_ok = h
                    .member()
                    .map(|m| match event {
                        SessionEvent::Lock => m.as_str() == "Lock",
                        SessionEvent::Unlock => m.as_str() == "Unlock",
                    })
                    .unwrap_or(false);

                if iface_ok && member_ok {
                    debug!("[session] Received {:?} signal", event);
                    let _ = event_tx.send(event);
                }
            }

            debug!("[session] Signal listener stopped for: {rule_str}");
        });

        Ok(())
    }
}
