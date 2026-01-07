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

use anyhow::{Context, Result};
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
    /// Connect to the session bus.
    pub async fn connect() -> Result<Self> {
        let conn = Connection::session()
            .await
            .context("Failed to connect to DBus session bus")?;

        Ok(Self {
            conn: Arc::new(conn),
        })
    }

    /// Read a DBus property as a `String` (best-effort).
    ///
    /// Notes:
    /// - This uses `org.freedesktop.DBus.Properties.Get`.
    /// - `destination` here is the **service name** you’re talking to (e.g. `nl.whynothugo.darkman`).
    /// - Returns `Ok(None)` if the property exists but is not a string.
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

        // Get(interface_name, property_name) -> (v)
        //
        // IMPORTANT: the first argument is the *interface name* that owns the property,
        // not the bus name. Many services use the same string for both; call sites should
        // pass the correct interface name.
        let (value,): (OwnedValue,) = proxy
            .call("Get", &(destination, property))
            .await
            .context("Failed to get property via DBus")?;

        // `Get` returns a DBus variant (as `OwnedValue`).
        Ok(owned_value_to_string(value))
    }

    /// Set a DBus property from a `&str` (best-effort).
    ///
    /// Notes:
    /// - This uses `org.freedesktop.DBus.Properties.Set`.
    /// - `destination` here is the **service name** you’re talking to.
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

        // Set(interface_name, property_name, v) -> ()
        //
        // The third parameter is a DBus variant. We pass a `zvariant::Value` which will be
        // marshalled as `v` by zbus.
        let v = Value::from(value.to_string());

        // Help type inference: `Set` returns `()`.
        let call_res: std::result::Result<(), _> =
            proxy.call("Set", &(destination, property, v)).await;

        call_res.context("Failed to set property via DBus")?;

        Ok(())
    }

    /// Listen for DBus signals matching a match rule, forwarding each raw `Message`
    /// into a `tokio::sync::broadcast` channel.
    ///
    /// This is intentionally low-level so feature modules can decode what they need.
    ///
    /// `match_rule` is a DBus match string like:
    /// - `type='signal',interface='nl.whynothugo.darkman',member='ModeChanged'`
    ///
    /// Implementation note:
    /// Prefer **bus-side filtering**:
    /// - Install the match on the bus via `org.freedesktop.DBus.AddMatch` (typed `MatchRule`).
    /// - Then consume incoming messages and forward only those that match (typically the bus will
    ///   only deliver matches once `AddMatch` succeeds).
    ///
    /// Fallback:
    /// - If we can't install the match rule (unexpected bus/proxy failure), we still listen to the
    ///   connection message stream and do conservative local filtering on interface/member.
    pub async fn listen_signals(&self, match_rule: &str) -> Result<broadcast::Receiver<Message>> {
        let (tx, rx) = broadcast::channel::<Message>(64);

        // Parse into a typed zbus match rule (this is what `AddMatch` expects).
        let rule: zbus::MatchRule<'static> = zbus::MatchRule::try_from(match_rule)
            .with_context(|| format!("Invalid DBus match rule: {match_rule}"))?
            .to_owned();

        // Best-effort bus-side match installation.
        let bus_side_installed = match zbus::fdo::DBusProxy::new(&*self.conn).await {
            Ok(dbus) => dbus.add_match_rule(rule.clone()).await.is_ok(),
            Err(_) => false,
        };

        let conn = self.conn.clone();
        let rule_str = match_rule.to_string();

        tokio::spawn(async move {
            // NOTE: `MessageStream` yields `Result<Message, zbus::Error>`.
            let mut stream = zbus::MessageStream::from(&*conn);

            while let Some(next) = stream.next().await {
                let msg = match next {
                    Ok(m) => m,
                    Err(_) => continue,
                };

                // If bus-side match installation worked, the bus should already be filtering.
                // Still, keep a cheap local filter as a safety net.
                if bus_side_installed {
                    let _ = tx.send(msg);
                    continue;
                }

                // Fallback local filter: only forward signals and match interface/member if present.
                let msg_type = msg.header().primary().msg_type();
                if msg_type as u8 != 4 {
                    continue;
                }

                let h = msg.header();

                let iface_req = rule_str.contains("interface='");
                let member_req = rule_str.contains("member='");

                let iface_ok = !iface_req
                    || h.interface()
                        .map(|i| rule_str.contains(&format!("interface='{}'", i)))
                        .unwrap_or(false);

                let member_ok = !member_req
                    || h.member()
                        .map(|m| rule_str.contains(&format!("member='{}'", m)))
                        .unwrap_or(false);

                if iface_ok && member_ok {
                    let _ = tx.send(msg);
                }
            }
        });

        Ok(rx)
    }

    /// Convenience helper to listen for a *single* string argument carried by a signal,
    /// calling `on_value(Some(String))` for each decoded value.
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
        });

        Ok(())
    }
}

/// Best-effort conversion of `OwnedValue` to `String`.
fn owned_value_to_string(v: OwnedValue) -> Option<String> {
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
