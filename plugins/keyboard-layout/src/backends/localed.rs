//! systemd-localed D-Bus keyboard layout backend.
//!
//! Uses D-Bus to communicate with org.freedesktop.locale1 service.
//!
//! ## D-Bus Interface
//!
//! - Property: `X11Layout` - Comma-separated list of configured XKB layouts
//! - Method: `SetX11Keyboard` - Change the keyboard layout configuration
//!
//! ## Limitations
//!
//! systemd-localed doesn't track which layout is "active" at runtime - it only
//! stores the configured layouts. This backend:
//! - Tracks current layout index locally
//! - Does NOT receive updates from external layout switches (e.g., keyboard shortcuts)
//! - Only receives updates when the configuration itself changes via D-Bus

use anyhow::{Context, Result};
use async_trait::async_trait;
use flume::Sender;
use futures_util::StreamExt;
use log::{debug, info, warn};
use std::sync::atomic::{AtomicUsize, Ordering};

use zbus::Connection;

use super::{KeyboardLayoutBackend, LayoutEvent, LayoutInfo};

const LOCALE1_SERVICE: &str = "org.freedesktop.locale1";
const LOCALE1_PATH: &str = "/org/freedesktop/locale1";
const LOCALE1_INTERFACE: &str = "org.freedesktop.locale1";

/// systemd-localed D-Bus keyboard layout backend.
pub struct LocaledBackend {
    conn: Connection,
    /// Track current layout index locally (localed doesn't track this)
    current_index: AtomicUsize,
}

impl LocaledBackend {
    /// Create a new localed backend.
    ///
    /// Returns `None` if the locale1 service is unavailable.
    pub async fn new(conn: Connection) -> Option<Self> {
        // Test connection by reading the property
        let layout = Self::read_x11_layout_property_with_conn(&conn).await.ok()?;

        if layout.is_empty() {
            debug!("[keyboard-layout:localed] X11Layout property not available or empty");
            return None;
        }

        info!("[keyboard-layout:localed] D-Bus client initialized");
        Some(Self {
            conn,
            current_index: AtomicUsize::new(0),
        })
    }

    /// Parse XKB layout string into uppercase abbreviations.
    ///
    /// Handles various XKB layout formats:
    /// - Simple layouts: "us" -> ["US"]
    /// - Multiple layouts: "us,de,fr" -> ["US", "DE", "FR"]
    /// - Layouts with variants: "us(dvorak)" -> ["US"]
    fn parse_xkb_layouts(layout_string: &str) -> Vec<String> {
        if layout_string.is_empty() {
            return Vec::new();
        }

        layout_string
            .split(',')
            .map(|layout| {
                // Remove variant information: "us(dvorak)" -> "us"
                let layout = layout.split('(').next().unwrap_or(layout);
                // Convert to uppercase: "us" -> "US"
                layout.trim().to_uppercase()
            })
            .filter(|layout| !layout.is_empty())
            .collect()
    }

    /// Read the X11Layout property using a given connection.
    async fn read_x11_layout_property_with_conn(conn: &Connection) -> Result<String> {
        let proxy = zbus::Proxy::new(
            conn,
            LOCALE1_SERVICE,
            LOCALE1_PATH,
            "org.freedesktop.DBus.Properties",
        )
        .await
        .context("Failed to create D-Bus proxy")?;

        let result: std::result::Result<(zbus::zvariant::OwnedValue,), _> =
            proxy.call("Get", &(LOCALE1_INTERFACE, "X11Layout")).await;

        match result {
            Ok((value,)) => {
                let val: zbus::zvariant::Value = value.into();
                if let zbus::zvariant::Value::Str(s) = val {
                    Ok(s.to_string())
                } else {
                    Ok(String::new())
                }
            }
            Err(_) => Ok(String::new()),
        }
    }

    /// Read the X11Layout property from org.freedesktop.locale1.
    async fn read_x11_layout_property(&self) -> Result<String> {
        Self::read_x11_layout_property_with_conn(&self.conn).await
    }

    /// Set the X11 keyboard layout.
    async fn set_layout(&self, layout: &str) -> Result<()> {
        let layout_lower = layout.to_lowercase();

        let proxy = zbus::Proxy::new(&self.conn, LOCALE1_SERVICE, LOCALE1_PATH, LOCALE1_INTERFACE)
            .await
            .context("Failed to create D-Bus proxy")?;

        let _: () = proxy
            .call(
                "SetX11Keyboard",
                &(
                    layout_lower.as_str(), // layout
                    "",                    // model (empty = keep current)
                    "",                    // variant (empty = keep current)
                    "",                    // options (empty = keep current)
                    false,                 // convert (don't convert to console keymap)
                    true,                  // interactive (allow PolicyKit prompts)
                ),
            )
            .await
            .context("Failed to set keyboard layout")?;

        info!("[keyboard-layout:localed] Layout set to: {}", layout);
        Ok(())
    }
}

#[async_trait]
impl KeyboardLayoutBackend for LocaledBackend {
    async fn get_layout_info(&self) -> Result<LayoutInfo> {
        let layout_string = self.read_x11_layout_property().await?;
        let available = Self::parse_xkb_layouts(&layout_string);

        if available.is_empty() {
            anyhow::bail!("No layouts configured in localed");
        }

        let current_index = self.current_index.load(Ordering::SeqCst) % available.len();
        let current = available
            .get(current_index)
            .cloned()
            .unwrap_or_else(|| "??".to_string());

        Ok(LayoutInfo {
            current,
            available,
            current_index,
        })
    }

    async fn switch_next(&self) -> Result<()> {
        let layout_string = self.read_x11_layout_property().await?;
        let available = Self::parse_xkb_layouts(&layout_string);

        if available.is_empty() {
            anyhow::bail!("No layouts available to cycle");
        }

        if available.len() == 1 {
            // Only one layout configured, cycling is a no-op
            return Ok(());
        }

        let current_index = self.current_index.load(Ordering::SeqCst) % available.len();
        let next_index = (current_index + 1) % available.len();
        let next_layout = &available[next_index];

        self.set_layout(next_layout).await?;
        self.current_index.store(next_index, Ordering::SeqCst);

        Ok(())
    }

    async fn switch_prev(&self) -> Result<()> {
        let layout_string = self.read_x11_layout_property().await?;
        let available = Self::parse_xkb_layouts(&layout_string);

        if available.is_empty() {
            anyhow::bail!("No layouts available to cycle");
        }

        if available.len() == 1 {
            // Only one layout configured, cycling is a no-op
            return Ok(());
        }

        let current_index = self.current_index.load(Ordering::SeqCst) % available.len();
        let prev_index = if current_index == 0 {
            available.len() - 1
        } else {
            current_index - 1
        };
        let prev_layout = &available[prev_index];

        self.set_layout(prev_layout).await?;
        self.current_index.store(prev_index, Ordering::SeqCst);

        Ok(())
    }

    fn name(&self) -> &'static str {
        "systemd-localed"
    }

    fn subscribe(&self, sender: Sender<LayoutEvent>) {
        // Note: systemd-localed only emits PropertiesChanged when the configuration
        // changes (e.g., via localectl or D-Bus SetX11Keyboard). It does NOT emit
        // events for runtime layout switches via keyboard shortcuts.
        //
        // This subscription will only catch configuration changes, not user input.
        warn!(
            "[keyboard-layout:localed] Note: systemd-localed backend does not support \
             live updates from external keyboard layout switches"
        );

        let conn = self.conn.clone();

        tokio::spawn(async move {
            debug!("[keyboard-layout:localed] Starting D-Bus property change subscription");

            // Subscribe to PropertiesChanged signal
            let rule = match zbus::MatchRule::builder()
                .msg_type(zbus::message::Type::Signal)
                .sender(LOCALE1_SERVICE)
                .and_then(|b| b.path(LOCALE1_PATH))
                .and_then(|b| b.interface("org.freedesktop.DBus.Properties"))
                .and_then(|b| b.member("PropertiesChanged"))
            {
                Ok(b) => b.build(),
                Err(e) => {
                    let _ = sender.send(LayoutEvent::Error(format!(
                        "Failed to build match rule: {e}"
                    )));
                    return;
                }
            };

            let dbus_proxy = match zbus::fdo::DBusProxy::new(&conn).await {
                Ok(p) => p,
                Err(e) => {
                    let _ = sender.send(LayoutEvent::Error(format!(
                        "Failed to create DBus proxy: {e}"
                    )));
                    return;
                }
            };

            if let Err(e) = dbus_proxy.add_match_rule(rule).await {
                let _ = sender.send(LayoutEvent::Error(format!("Failed to add match rule: {e}")));
                return;
            }

            info!("[keyboard-layout:localed] Listening for PropertiesChanged signals");

            let mut stream = zbus::MessageStream::from(&conn);
            while let Some(msg) = stream.next().await {
                let msg = match msg {
                    Ok(m) => m,
                    Err(e) => {
                        warn!("[keyboard-layout:localed] D-Bus stream error: {}", e);
                        continue;
                    }
                };

                let header = msg.header();
                if header.member().map(|m| m.as_str()) != Some("PropertiesChanged")
                    || header.interface().map(|i| i.as_str())
                        != Some("org.freedesktop.DBus.Properties")
                {
                    continue;
                }

                // Parse PropertiesChanged body:
                // (interface_name, changed_properties, invalidated_properties)
                if let Ok((iface, changed, _invalidated)) = msg.body().deserialize::<(
                    String,
                    std::collections::HashMap<String, zbus::zvariant::OwnedValue>,
                    Vec<String>,
                )>() && iface == LOCALE1_INTERFACE
                    && let Some(value) = changed.get("X11Layout")
                    && let Ok(layout_str) = <String>::try_from(value.clone())
                {
                    debug!(
                        "[keyboard-layout:localed] Configuration changed: {}",
                        layout_str
                    );
                    let available = Self::parse_xkb_layouts(&layout_str);
                    if !available.is_empty() {
                        let current = available[0].clone();
                        let info = LayoutInfo {
                            current,
                            available,
                            current_index: 0,
                        };
                        if sender.send(LayoutEvent::Changed(info)).is_err() {
                            break;
                        }
                    }
                }
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_xkb_simple_layout() {
        let result = LocaledBackend::parse_xkb_layouts("us");
        assert_eq!(result, vec!["US"]);
    }

    #[test]
    fn test_parse_xkb_multi_layout() {
        let result = LocaledBackend::parse_xkb_layouts("us,de,fr");
        assert_eq!(result, vec!["US", "DE", "FR"]);
    }

    #[test]
    fn test_parse_xkb_layout_with_variant() {
        let result = LocaledBackend::parse_xkb_layouts("us(dvorak)");
        assert_eq!(result, vec!["US"]);
    }

    #[test]
    fn test_parse_xkb_empty_string() {
        let result = LocaledBackend::parse_xkb_layouts("");
        assert_eq!(result, Vec::<String>::new());
    }

    #[test]
    fn test_parse_xkb_multi_with_variants() {
        let result = LocaledBackend::parse_xkb_layouts("us(dvorak),de(nodeadkeys),cz(qwerty)");
        assert_eq!(result, vec!["US", "DE", "CZ"]);
    }
}
