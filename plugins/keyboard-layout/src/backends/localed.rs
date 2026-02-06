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
use log::{debug, info, warn};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use waft_core::dbus::DbusHandle;

use super::{KeyboardLayoutBackend, LayoutEvent, LayoutInfo};

const LOCALE1_SERVICE: &str = "org.freedesktop.locale1";
const LOCALE1_PATH: &str = "/org/freedesktop/locale1";
const LOCALE1_INTERFACE: &str = "org.freedesktop.locale1";

/// systemd-localed D-Bus keyboard layout backend.
pub struct LocaledBackend {
    dbus: Arc<DbusHandle>,
    /// Track current layout index locally (localed doesn't track this)
    current_index: AtomicUsize,
}

impl LocaledBackend {
    /// Create a new localed backend.
    ///
    /// Returns `None` if the locale1 service is unavailable.
    pub async fn new(dbus: Arc<DbusHandle>) -> Option<Self> {
        // Test connection by reading the property
        let layout = dbus
            .get_property(LOCALE1_SERVICE, LOCALE1_PATH, "X11Layout")
            .await
            .ok()?;

        if layout.is_none() {
            debug!("[keyboard-layout:localed] X11Layout property not available");
            return None;
        }

        info!("[keyboard-layout:localed] D-Bus client initialized");
        Some(Self {
            dbus,
            current_index: AtomicUsize::new(0),
        })
    }

    /// Parse XKB layout string into uppercase abbreviations.
    ///
    /// Handles various XKB layout formats:
    /// - Simple layouts: "us" → ["US"]
    /// - Multiple layouts: "us,de,fr" → ["US", "DE", "FR"]
    /// - Layouts with variants: "us(dvorak)" → ["US"]
    fn parse_xkb_layouts(layout_string: &str) -> Vec<String> {
        if layout_string.is_empty() {
            return Vec::new();
        }

        layout_string
            .split(',')
            .map(|layout| {
                // Remove variant information: "us(dvorak)" → "us"
                let layout = layout.split('(').next().unwrap_or(layout);
                // Convert to uppercase: "us" → "US"
                layout.trim().to_uppercase()
            })
            .filter(|layout| !layout.is_empty())
            .collect()
    }

    /// Read the X11Layout property from org.freedesktop.locale1.
    async fn read_x11_layout_property(&self) -> Result<String> {
        let layout = self
            .dbus
            .get_property(LOCALE1_SERVICE, LOCALE1_PATH, "X11Layout")
            .await
            .context("Failed to read X11Layout property")?;

        Ok(layout.unwrap_or_default())
    }

    /// Set the X11 keyboard layout.
    async fn set_layout(&self, layout: &str) -> Result<()> {
        let layout_lower = layout.to_lowercase();

        self.dbus
            .connection()
            .call_method(
                Some(LOCALE1_SERVICE),
                LOCALE1_PATH,
                Some(LOCALE1_INTERFACE),
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

        let dbus = self.dbus.clone();

        tokio::spawn(async move {
            debug!("[keyboard-layout:localed] Starting D-Bus property change subscription");

            // Clone sender for use in the error case
            let sender_for_error = sender.clone();

            let result = dbus
                .listen_properties_changed(
                    LOCALE1_SERVICE,
                    LOCALE1_PATH,
                    LOCALE1_INTERFACE,
                    move |_interface, changed| {
                        // Check if X11Layout changed
                        if let Some(value) = changed.get("X11Layout")
                            && let Ok(layout_str) = <String>::try_from(value.clone()) {
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
                                    let _ = sender.send(LayoutEvent::Changed(info));
                                }
                            }
                    },
                )
                .await;

            if let Err(e) = result {
                let _ = sender_for_error.send(LayoutEvent::Error(format!(
                    "Failed to subscribe to localed changes: {e}"
                )));
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
