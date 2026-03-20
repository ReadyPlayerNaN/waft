//! D-Bus signal monitoring helpers for plugins.
//!
//! Provides reusable infrastructure for monitoring D-Bus signals and updating
//! plugin state when external changes occur.

use anyhow::{Context, Result};
use futures_util::StreamExt;
use std::sync::{Arc, Mutex as StdMutex};
use zbus::{Connection, MatchRule, MessageStream};

use crate::notifier::EntityNotifier;

/// Configuration for monitoring a D-Bus signal.
#[derive(Debug, Clone)]
pub struct SignalMonitorConfig {
    /// D-Bus service name (sender).
    pub sender: String,
    /// Object path to monitor.
    pub path: String,
    /// Interface name.
    pub interface: String,
    /// Signal member name.
    pub member: String,
}

impl SignalMonitorConfig {
    /// Create a new builder.
    pub fn builder() -> SignalMonitorConfigBuilder {
        SignalMonitorConfigBuilder::default()
    }

    fn build_match_rule(&self) -> Result<MatchRule<'_>> {
        Ok(MatchRule::builder()
            .msg_type(zbus::message::Type::Signal)
            .sender(self.sender.as_str())?
            .path(self.path.as_str())?
            .interface(self.interface.as_str())?
            .member(self.member.as_str())?
            .build())
    }
}

/// Builder for `SignalMonitorConfig`.
#[derive(Default)]
pub struct SignalMonitorConfigBuilder {
    sender: Option<String>,
    path: Option<String>,
    interface: Option<String>,
    member: Option<String>,
}

impl SignalMonitorConfigBuilder {
    pub fn sender(mut self, sender: impl Into<String>) -> Self {
        self.sender = Some(sender.into());
        self
    }

    pub fn path(mut self, path: impl Into<String>) -> Self {
        self.path = Some(path.into());
        self
    }

    pub fn interface(mut self, interface: impl Into<String>) -> Self {
        self.interface = Some(interface.into());
        self
    }

    pub fn member(mut self, member: impl Into<String>) -> Self {
        self.member = Some(member.into());
        self
    }

    pub fn build(self) -> Result<SignalMonitorConfig> {
        Ok(SignalMonitorConfig {
            sender: self.sender.context("sender is required")?,
            path: self.path.context("path is required")?,
            interface: self.interface.context("interface is required")?,
            member: self.member.context("member is required")?,
        })
    }
}

/// Monitor a D-Bus signal and call a handler when the signal is received.
///
/// The handler receives the zbus message and a mutable reference to the state
/// (already locked). It should return `Ok(true)` to trigger an entity update
/// via the notifier, or `Ok(false)` to skip.
pub async fn monitor_signal<T, F>(
    conn: Connection,
    config: SignalMonitorConfig,
    state: Arc<StdMutex<T>>,
    notifier: EntityNotifier,
    mut handler: F,
) -> Result<()>
where
    T: Send + 'static,
    F: FnMut(&zbus::Message, &mut T) -> Result<bool> + Send + 'static,
{
    let rule = config
        .build_match_rule()
        .context("failed to build match rule")?;

    let dbus_proxy = zbus::fdo::DBusProxy::new(&conn)
        .await
        .context("failed to create DBus proxy")?;

    dbus_proxy
        .add_match_rule(rule)
        .await
        .context("failed to add match rule")?;

    log::info!(
        "Listening for D-Bus signals: {}.{}",
        config.interface,
        config.member
    );

    let mut stream = MessageStream::from(&conn);
    while let Some(msg) = stream.next().await {
        let msg = match msg {
            Ok(m) => m,
            Err(e) => {
                log::warn!("D-Bus stream error: {e}");
                continue;
            }
        };

        let header = msg.header();
        if header.member().map(|m| m.as_str()) == Some(&config.member)
            && header.interface().map(|i| i.as_str()) == Some(&config.interface)
        {
            let mut state_guard = crate::lock_or_recover(&state);

            match handler(&msg, &mut *state_guard) {
                Ok(should_notify) => {
                    if should_notify {
                        notifier.notify();
                    }
                }
                Err(e) => {
                    log::warn!(
                        "Failed to process {}.{} signal: {e}",
                        config.interface,
                        config.member,
                    );
                }
            }
        }
    }

    log::warn!(
        "D-Bus signal stream ended for {}.{}",
        config.interface,
        config.member
    );
    Ok(())
}

/// Monitor a D-Bus signal with an async handler.
///
/// Unlike `monitor_signal`, the handler is async and does not hold the
/// state lock while running. The handler returns `Ok(Some(new_state))`
/// to update state and notify, or `Ok(None)` to skip.
pub async fn monitor_signal_async<T, F, Fut>(
    conn: Connection,
    config: SignalMonitorConfig,
    state: Arc<StdMutex<T>>,
    notifier: EntityNotifier,
    mut handler: F,
) -> Result<()>
where
    T: Send + 'static,
    F: FnMut(&zbus::Message, Arc<StdMutex<T>>) -> Fut + Send + 'static,
    Fut: std::future::Future<Output = Result<Option<T>>> + Send + 'static,
{
    let rule = config
        .build_match_rule()
        .context("failed to build match rule")?;

    let dbus_proxy = zbus::fdo::DBusProxy::new(&conn)
        .await
        .context("failed to create DBus proxy")?;

    dbus_proxy
        .add_match_rule(rule)
        .await
        .context("failed to add match rule")?;

    log::info!(
        "Listening for D-Bus signals: {}.{}",
        config.interface,
        config.member
    );

    let mut stream = MessageStream::from(&conn);
    while let Some(msg) = stream.next().await {
        let msg = match msg {
            Ok(m) => m,
            Err(e) => {
                log::warn!("D-Bus stream error: {e}");
                continue;
            }
        };

        let header = msg.header();
        if header.member().map(|m| m.as_str()) == Some(&config.member)
            && header.interface().map(|i| i.as_str()) == Some(&config.interface)
        {
            match handler(&msg, state.clone()).await {
                Ok(Some(new_state)) => {
                    *crate::lock_or_recover(&state) = new_state;
                    notifier.notify();
                }
                Ok(None) => {}
                Err(e) => {
                    log::warn!(
                        "Failed to process {}.{} signal: {e}",
                        config.interface,
                        config.member,
                    );
                }
            }
        }
    }

    log::warn!(
        "D-Bus signal stream ended for {}.{}",
        config.interface,
        config.member
    );
    Ok(())
}
