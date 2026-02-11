//! D-Bus signal monitoring helpers for daemon plugins.
//!
//! Provides reusable infrastructure for monitoring D-Bus signals and updating
//! plugin state when external changes occur.

use anyhow::{Context, Result};
use futures_util::StreamExt;
use std::sync::{Arc, Mutex as StdMutex};
use zbus::{Connection, MatchRule, MessageStream};

use crate::server::WidgetNotifier;

/// Configuration for monitoring a D-Bus signal.
///
/// # Example
///
/// ```rust,no_run
/// use waft_plugin_sdk::dbus_monitor::SignalMonitorConfig;
///
/// let config = SignalMonitorConfig::builder()
///     .sender("nl.whynothugo.darkman")
///     .path("/nl/whynothugo/darkman")
///     .interface("nl.whynothugo.darkman")
///     .member("ModeChanged")
///     .build()
///     .unwrap();
/// ```
#[derive(Debug, Clone)]
pub struct SignalMonitorConfig {
    /// D-Bus service name (sender)
    pub sender: String,
    /// Object path to monitor
    pub path: String,
    /// Interface name
    pub interface: String,
    /// Signal member name
    pub member: String,
}

impl SignalMonitorConfig {
    /// Create a new builder for signal monitor configuration.
    pub fn builder() -> SignalMonitorConfigBuilder {
        SignalMonitorConfigBuilder::default()
    }

    /// Build a zbus MatchRule from this configuration.
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
    /// Set the D-Bus service name (sender).
    pub fn sender(mut self, sender: impl Into<String>) -> Self {
        self.sender = Some(sender.into());
        self
    }

    /// Set the object path to monitor.
    pub fn path(mut self, path: impl Into<String>) -> Self {
        self.path = Some(path.into());
        self
    }

    /// Set the interface name.
    pub fn interface(mut self, interface: impl Into<String>) -> Self {
        self.interface = Some(interface.into());
        self
    }

    /// Set the signal member name.
    pub fn member(mut self, member: impl Into<String>) -> Self {
        self.member = Some(member.into());
        self
    }

    /// Build the configuration.
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
/// This function sets up signal monitoring with the following features:
/// - Subscribes to the specified signal via D-Bus match rules
/// - Filters messages by interface and member
/// - Deserializes signal body using the provided handler
/// - Automatically handles stream errors with logging
/// - Never polls - waits for signals to arrive
///
/// # Type Parameters
///
/// - `F`: Handler function that processes signal body and updates state
///
/// # Arguments
///
/// - `conn`: D-Bus connection (session or system bus)
/// - `config`: Signal monitoring configuration
/// - `state`: Shared state to update (Arc<Mutex<T>>)
/// - `notifier`: Widget notifier to trigger UI updates
/// - `handler`: Function to process signal body and update state
///
/// # Handler Function
///
/// The handler receives:
/// - `msg`: The zbus message
/// - `state`: Mutable reference to the state (already locked)
///
/// It should:
/// - Deserialize the message body
/// - Update the state accordingly
/// - Return `Ok(true)` to trigger a widget update, or `Ok(false)` to skip
///
/// # Example
///
/// ```rust,no_run
/// use std::sync::{Arc, Mutex as StdMutex};
/// use waft_plugin_sdk::dbus_monitor::{monitor_signal, SignalMonitorConfig};
///
/// #[derive(Default)]
/// struct State {
///     mode: String,
/// }
///
/// async fn example(
///     conn: zbus::Connection,
///     state: Arc<StdMutex<State>>,
///     notifier: waft_plugin_sdk::WidgetNotifier,
/// ) -> anyhow::Result<()> {
///     let config = SignalMonitorConfig::builder()
///         .sender("nl.whynothugo.darkman")
///         .path("/nl/whynothugo/darkman")
///         .interface("nl.whynothugo.darkman")
///         .member("ModeChanged")
///         .build()?;
///
///     monitor_signal(conn, config, state, notifier, |msg, state| {
///         let new_mode: String = msg.body().deserialize()?;
///         state.mode = new_mode;
///         Ok(true) // Trigger widget update
///     }).await
/// }
/// ```
pub async fn monitor_signal<T, F>(
    conn: Connection,
    config: SignalMonitorConfig,
    state: Arc<StdMutex<T>>,
    notifier: WidgetNotifier,
    mut handler: F,
) -> Result<()>
where
    T: Send + 'static,
    F: FnMut(&zbus::Message, &mut T) -> Result<bool> + Send + 'static,
{
    // Build match rule
    let rule = config
        .build_match_rule()
        .context("Failed to build match rule")?;

    // Subscribe to signal
    let dbus_proxy = zbus::fdo::DBusProxy::new(&conn)
        .await
        .context("Failed to create DBus proxy")?;

    dbus_proxy
        .add_match_rule(rule)
        .await
        .context("Failed to add match rule")?;

    log::info!(
        "Listening for D-Bus signals: {}.{}",
        config.interface,
        config.member
    );

    // Process signal stream
    let mut stream = MessageStream::from(&conn);
    while let Some(msg) = stream.next().await {
        let msg = match msg {
            Ok(m) => m,
            Err(e) => {
                log::warn!("D-Bus stream error: {}", e);
                continue;
            }
        };

        // Filter by interface and member
        let header = msg.header();
        if header.member().map(|m| m.as_str()) == Some(&config.member)
            && header.interface().map(|i| i.as_str()) == Some(&config.interface)
        {
            // Call handler with locked state
            let mut state_guard = match state.lock() {
                Ok(g) => g,
                Err(e) => {
                    log::warn!("Mutex poisoned, recovering: {}", e);
                    e.into_inner()
                }
            };

            match handler(&msg, &mut *state_guard) {
                Ok(should_notify) => {
                    if should_notify {
                        notifier.notify();
                    }
                }
                Err(e) => {
                    log::warn!(
                        "Failed to process {}.{} signal: {}",
                        config.interface,
                        config.member,
                        e
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
/// This is similar to `monitor_signal` but allows the handler to be async,
/// which is useful when the handler needs to make async D-Bus calls or perform
/// other async operations.
///
/// Unlike `monitor_signal`, the async handler receives the message without
/// holding the state lock, allowing it to perform async operations before
/// updating the state. The handler returns an optional new state value.
///
/// # Type Parameters
///
/// - `F`: Async handler function that processes signal body and returns new state
///
/// # Arguments
///
/// - `conn`: D-Bus connection (session or system bus)
/// - `config`: Signal monitoring configuration
/// - `state`: Shared state to update (Arc<Mutex<T>>)
/// - `notifier`: Widget notifier to trigger UI updates
/// - `handler`: Async function to process signal body and compute new state
///
/// # Handler Function
///
/// The handler receives:
/// - `msg`: The zbus message (must deserialize before async operations)
/// - `state`: Arc to the shared state (for read access if needed)
///
/// It should:
/// - Deserialize the message body synchronously
/// - Perform async operations (D-Bus calls, etc.)
/// - Return `Ok(Some(new_state))` to update state and trigger UI, or `Ok(None)` to skip
///
/// # Example
///
/// ```rust,no_run
/// use std::sync::{Arc, Mutex as StdMutex};
/// use waft_plugin_sdk::dbus_monitor::{monitor_signal_async, SignalMonitorConfig};
///
/// #[derive(Default, Clone)]
/// struct BatteryInfo {
///     percentage: f64,
/// }
///
/// async fn fetch_battery_info(conn: &zbus::Connection) -> anyhow::Result<BatteryInfo> {
///     // Async D-Bus call
///     Ok(BatteryInfo { percentage: 75.0 })
/// }
///
/// async fn example(
///     conn: zbus::Connection,
///     state: Arc<StdMutex<BatteryInfo>>,
///     notifier: waft_plugin_sdk::WidgetNotifier,
/// ) -> anyhow::Result<()> {
///     let config = SignalMonitorConfig::builder()
///         .sender("org.freedesktop.DBus")
///         .path("/org/freedesktop/UPower/devices/DisplayDevice")
///         .interface("org.freedesktop.DBus.Properties")
///         .member("PropertiesChanged")
///         .build()?;
///
///     let conn_clone = conn.clone();
///     monitor_signal_async(conn, config, state, notifier, move |msg, _state| {
///         // Deserialize BEFORE entering the async block (msg is borrowed)
///         let iface_result = msg.body().deserialize::<(String,)>();
///
///         let conn = conn_clone.clone();
///         Box::pin(async move {
///             let (iface_name,) = iface_result?;
///
///             // Now we can await
///             let new_info = fetch_battery_info(&conn).await?;
///             Ok(Some(new_info))
///         })
///     }).await
/// }
/// ```
pub async fn monitor_signal_async<T, F, Fut>(
    conn: Connection,
    config: SignalMonitorConfig,
    state: Arc<StdMutex<T>>,
    notifier: WidgetNotifier,
    mut handler: F,
) -> Result<()>
where
    T: Send + 'static,
    F: FnMut(&zbus::Message, Arc<StdMutex<T>>) -> Fut + Send + 'static,
    Fut: std::future::Future<Output = Result<Option<T>>> + Send + 'static,
{
    // Build match rule
    let rule = config
        .build_match_rule()
        .context("Failed to build match rule")?;

    // Subscribe to signal
    let dbus_proxy = zbus::fdo::DBusProxy::new(&conn)
        .await
        .context("Failed to create DBus proxy")?;

    dbus_proxy
        .add_match_rule(rule)
        .await
        .context("Failed to add match rule")?;

    log::info!(
        "Listening for D-Bus signals: {}.{}",
        config.interface,
        config.member
    );

    // Process signal stream
    let mut stream = MessageStream::from(&conn);
    while let Some(msg) = stream.next().await {
        let msg = match msg {
            Ok(m) => m,
            Err(e) => {
                log::warn!("D-Bus stream error: {}", e);
                continue;
            }
        };

        // Filter by interface and member
        let header = msg.header();
        if header.member().map(|m| m.as_str()) == Some(&config.member)
            && header.interface().map(|i| i.as_str()) == Some(&config.interface)
        {
            // Call handler (without holding lock)
            match handler(&msg, state.clone()).await {
                Ok(Some(new_state)) => {
                    // Update state and notify
                    match state.lock() {
                        Ok(mut guard) => {
                            *guard = new_state;
                            notifier.notify();
                        }
                        Err(e) => {
                            log::warn!("Mutex poisoned, recovering: {}", e);
                            *e.into_inner() = new_state;
                            notifier.notify();
                        }
                    }
                }
                Ok(None) => {
                    // Handler chose to skip this signal
                }
                Err(e) => {
                    log::warn!(
                        "Failed to process {}.{} signal: {}",
                        config.interface,
                        config.member,
                        e
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
