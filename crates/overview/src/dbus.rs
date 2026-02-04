//! DBus client wrapper built on `zbus`.
//!
//! This module is the **client-side** DBus helper layer.
//! The notifications DBus *server* lives in `crate::notifications_dbus_server` (also `zbus`).
//!
//! Goals:
//! - Provide a small, cloneable handle for DBus client calls.
//! - Provide property get/set via the standard `org.freedesktop.DBus.Properties` interface.
//! - Provide signal listening using DBus match rules + a background receiver task.
//!
//! Threading model:
//! - DBus IO is async and intended to run on background tasks (`tokio::spawn`).
//! - Do not touch GTK from any of these callbacks; forward state changes to the UI via channels.
//!
//! Buses:
//! - Most app integrations use the **session bus**.
//! - Some system services (e.g. BlueZ) live on the **system bus**.

use anyhow::{Context, Result};
use log::debug;
use std::sync::Arc;

use tokio::sync::broadcast;

use zbus::{Connection, Message};
use zvariant::{OwnedValue, Value};

/// Shared async DBus session-bus connection (zbus).
#[derive(Clone)]
pub struct DbusHandle {
    conn: Arc<Connection>,
}

impl DbusHandle {
    /// Access the underlying zbus connection for advanced integrations
    /// that need typed proxies (e.g. BlueZ ObjectManager).
    pub fn connection(&self) -> Arc<Connection> {
        self.conn.clone()
    }
}

impl DbusHandle {
    /// Connect to the session bus.
    pub async fn connect() -> Result<Self> {
        let conn = Connection::session()
            .await
            .context("Failed to connect to DBus session bus")?;

        Ok(Self {
            conn: Arc::new(conn),
        })
    }

    /// Connect to the system bus. Use for system services like BlueZ.
    pub async fn connect_system() -> Result<Self> {
        let conn = Connection::system()
            .await
            .context("Failed to connect to DBus system bus")?;

        Ok(Self {
            conn: Arc::new(conn),
        })
    }

    /// Get a string property via org.freedesktop.DBus.Properties.Get.
    /// Returns None if property doesn't exist or isn't a string.
    pub async fn get_property(
        &self,
        destination: &str,
        path: &str,
        property: &str,
    ) -> Result<Option<String>> {
        let proxy = zbus::Proxy::new(
            &*self.conn,
            destination,
            path,
            "org.freedesktop.DBus.Properties",
        )
        .await
        .context("Failed to create DBus Properties proxy")?;

        // Note: destination is interface name, not bus name (often the same)
        let (value,): (OwnedValue,) = proxy
            .call("Get", &(destination, property))
            .await
            .context("Failed to get property via DBus")?;

        Ok(owned_value_to_string(value))
    }

    /// Set a string property via org.freedesktop.DBus.Properties.Set.
    pub async fn set_property(
        &self,
        destination: &str,
        path: &str,
        property: &str,
        value: &str,
    ) -> Result<()> {
        let proxy = zbus::Proxy::new(
            &*self.conn,
            destination,
            path,
            "org.freedesktop.DBus.Properties",
        )
        .await
        .context("Failed to create DBus Properties proxy")?;

        let v = Value::from(value.to_string());
        let call_res: std::result::Result<(), _> =
            proxy.call("Set", &(destination, property, v)).await;

        call_res.context("Failed to set property via DBus")?;

        Ok(())
    }

    /// Get all properties via org.freedesktop.DBus.Properties.GetAll.
    /// Returns HashMap of property names to values.
    pub async fn get_all_properties(
        &self,
        destination: &str,
        path: &str,
        interface: &str,
    ) -> Result<std::collections::HashMap<String, OwnedValue>> {
        let proxy = zbus::Proxy::new(
            &*self.conn,
            destination,
            path,
            "org.freedesktop.DBus.Properties",
        )
        .await
        .context("Failed to create DBus Properties proxy")?;

        let (props,): (std::collections::HashMap<String, OwnedValue>,) = proxy
            .call("GetAll", &(interface,))
            .await
            .context("Failed to call GetAll on DBus Properties")?;

        Ok(props)
    }

    /// Extract a typed property from a properties HashMap.
    ///
    /// Attempts to extract and convert the property value using TryFrom.
    /// Returns the converted value on success, or the provided default on failure.
    ///
    /// # Example
    /// ```ignore
    /// let available = DbusHandle::extract_property::<bool>(&props, "Available", false);
    /// let percentage = DbusHandle::extract_property::<f64>(&props, "Percentage", 0.0);
    /// ```
    pub fn extract_property<T>(
        props: &std::collections::HashMap<String, OwnedValue>,
        property_name: &str,
        default: T,
    ) -> T
    where
        T: TryFrom<OwnedValue>,
    {
        props
            .get(property_name)
            .and_then(|v| T::try_from(v.clone()).ok())
            .unwrap_or(default)
    }

    /// Extract a property from a HashMap, trying multiple property names in order.
    ///
    /// Useful when a property may have multiple names (e.g., "Alias" or "Name").
    /// Returns the first property found that successfully converts, or the default.
    ///
    /// # Example
    /// ```ignore
    /// let name = DbusHandle::extract_property_or::<String>(
    ///     &props,
    ///     &["Alias", "Name"],
    ///     "Unknown".to_string()
    /// );
    /// ```
    pub fn extract_property_or<T>(
        props: &std::collections::HashMap<String, OwnedValue>,
        property_names: &[&str],
        default: T,
    ) -> T
    where
        T: TryFrom<OwnedValue>,
    {
        for name in property_names {
            if let Some(v) = props.get(*name) {
                if let Ok(value) = T::try_from(v.clone()) {
                    return value;
                }
            }
        }
        default
    }

    /// Listen for PropertiesChanged signals on a specific interface.
    /// Calls callback with interface name and changed properties HashMap.
    pub async fn listen_properties_changed(
        &self,
        destination: &str,
        path: &str,
        interface: &str,
        mut on_change: impl FnMut(String, std::collections::HashMap<String, OwnedValue>)
        + Send
        + 'static,
    ) -> Result<()> {
        let rule = format!(
            "type='signal',interface='org.freedesktop.DBus.Properties',member='PropertiesChanged',path='{}',sender='{}'",
            escape_match_value(path),
            escape_match_value(destination)
        );

        let mut rx = self.listen_signals(&rule).await?;
        let filter_interface = interface.to_string();

        tokio::spawn(async move {
            loop {
                match rx.recv().await {
                    Ok(msg) => {
                        // Parse PropertiesChanged signal body:
                        // (interface_name, changed_properties, invalidated_properties)
                        if let Ok((iface, changed, _invalidated)) = msg.body().deserialize::<(
                            String,
                            std::collections::HashMap<String, OwnedValue>,
                            Vec<String>,
                        )>(
                        ) {
                            // Only process changes for our target interface
                            if iface == filter_interface {
                                on_change(iface, changed);
                            }
                        }
                    }
                    Err(broadcast::error::RecvError::Lagged(_)) => continue,
                    Err(broadcast::error::RecvError::Closed) => break,
                }
            }
            debug!("[dbus] properties changed listener stopped");
        });

        Ok(())
    }

    /// Listen for DBus signals matching a match rule.
    /// Returns broadcast receiver for matched messages.
    /// Uses bus-side filtering when possible, with local filtering fallback.
    pub async fn listen_signals(&self, match_rule: &str) -> Result<broadcast::Receiver<Message>> {
        let (tx, rx) = broadcast::channel::<Message>(64);

        let rule: zbus::MatchRule<'static> = zbus::MatchRule::try_from(match_rule)
            .with_context(|| format!("Invalid DBus match rule: {match_rule}"))?
            .to_owned();

        // Best-effort bus-side match installation (local filtering always applied)
        let _ = match zbus::fdo::DBusProxy::new(&*self.conn).await {
            Ok(dbus) => dbus.add_match_rule(rule.clone()).await.is_ok(),
            Err(_) => false,
        };

        let conn = self.conn.clone();
        let rule_str = match_rule.to_string();

        tokio::spawn(async move {
            let mut stream = zbus::MessageStream::from(&*conn);

            while let Some(next) = stream.next().await {
                let msg = match next {
                    Ok(m) => m,
                    Err(e) => {
                        debug!("[dbus] signal stream error: {e}");
                        continue;
                    }
                };

                // Always filter locally (MessageStream receives all message types)
                // Only process signal messages (type=4)
                let msg_type = msg.header().primary().msg_type();
                if msg_type as u8 != 4 {
                    continue;
                }

                let h = msg.header();

                // Filter by interface if specified in match rule
                let iface_req = rule_str.contains("interface='");
                let iface_ok = !iface_req
                    || h.interface()
                        .map(|i| rule_str.contains(&format!("interface='{}'", i)))
                        .unwrap_or(false);

                // Filter by member if specified in match rule
                let member_req = rule_str.contains("member='");
                let member_ok = !member_req
                    || h.member()
                        .map(|m| rule_str.contains(&format!("member='{}'", m)))
                        .unwrap_or(false);

                if iface_ok && member_ok {
                    if tx.send(msg).is_err() {
                        break;
                    }
                }
            }
            debug!("[dbus] signal listener stopped for rule: {rule_str}");
        });

        Ok(rx)
    }

    /// Listen for signals with a single string argument.
    /// Calls callback with decoded string for each signal.
    pub async fn listen_for_values(
        &self,
        interface: &str,
        member: &str,
        mut on_value: impl FnMut(Option<String>) + Send + 'static,
    ) -> Result<()> {
        let rule = format!(
            "type='signal',interface='{}',member='{}'",
            escape_match_value(interface),
            escape_match_value(member)
        );

        let mut rx = self.listen_signals(&rule).await?;

        tokio::spawn(async move {
            loop {
                match rx.recv().await {
                    Ok(msg) => on_value(decode_first_body_string(&msg)),
                    Err(broadcast::error::RecvError::Lagged(_)) => continue,
                    Err(broadcast::error::RecvError::Closed) => break,
                }
            }
            debug!("[dbus] value listener stopped");
        });

        Ok(())
    }
}

/// Best-effort conversion of `OwnedValue` to `String`.
pub fn owned_value_to_string(v: OwnedValue) -> Option<String> {
    let val: Value = v.into();
    if let Value::Str(s) = val {
        return Some(s.to_string());
    }
    None
}

/// Try to decode the first body field of a message into a string.
fn decode_first_body_string(msg: &Message) -> Option<String> {
    if let Ok((s,)) = msg.body().deserialize::<(String,)>() {
        return Some(s);
    }
    if let Ok((v,)) = msg.body().deserialize::<(OwnedValue,)>() {
        return owned_value_to_string(v);
    }
    None
}

/// Escape a string for inclusion in a DBus match rule value.
fn escape_match_value(s: &str) -> String {
    s.replace('\'', "\\'")
}

use futures_util::StreamExt;

#[cfg(test)]
#[path = "dbus_tests.rs"]
mod tests;
