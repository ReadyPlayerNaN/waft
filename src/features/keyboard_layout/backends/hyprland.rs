//! Hyprland compositor keyboard layout backend.
//!
//! Uses `hyprctl` command for layout queries and switching.
//!
//! ## Commands
//!
//! - Query: `hyprctl devices -j`
//! - Switch: `hyprctl switchxkblayout all next`
//!
//! ## Event Socket
//!
//! Hyprland uses a socket at `$XDG_RUNTIME_DIR/hypr/$HYPRLAND_INSTANCE_SIGNATURE/.socket2.sock`
//! for IPC events. Layout change events have the format: `activelayout>>keyboard_name,layout_name`

use anyhow::{Context, Result};
use async_trait::async_trait;
use flume::Sender;
use log::{debug, warn};
use serde::Deserialize;
use tokio::io::AsyncBufReadExt;
use tokio::net::UnixStream;
use tokio::process::Command;

use super::{extract_abbreviation, KeyboardLayoutBackend, LayoutEvent, LayoutInfo};

/// Hyprland devices response from `hyprctl devices -j`.
#[derive(Debug, Deserialize)]
struct HyprlandDevices {
    keyboards: Vec<HyprlandKeyboard>,
}

/// Hyprland keyboard device.
#[derive(Debug, Deserialize)]
struct HyprlandKeyboard {
    /// Keyboard name/identifier
    #[allow(dead_code)]
    name: String,
    /// Active keymap name (e.g., "English (US)")
    active_keymap: String,
    /// Whether this is the main keyboard
    main: bool,
}

/// Hyprland compositor keyboard layout backend.
pub struct HyprlandBackend {
    /// Cached layout names from config (Hyprland doesn't report all layouts in devices)
    layout_names: Vec<String>,
}

impl HyprlandBackend {
    /// Create a new Hyprland backend.
    ///
    /// Returns `None` if the `hyprctl` command is not available.
    pub async fn new() -> Option<Self> {
        // Verify hyprctl command is available
        let output = Command::new("hyprctl")
            .arg("version")
            .output()
            .await
            .ok()?;

        if !output.status.success() {
            warn!("[keyboard-layout:hyprland] Hyprctl command not available");
            return None;
        }

        debug!("[keyboard-layout:hyprland] Hyprctl command available");

        // Try to get layout list from hyprctl getoption
        let layout_names = Self::get_configured_layouts().await.unwrap_or_default();

        Some(Self { layout_names })
    }

    /// Get configured layouts from Hyprland config via hyprctl.
    async fn get_configured_layouts() -> Result<Vec<String>> {
        // Try to get the kb_layout option
        let output = Command::new("hyprctl")
            .args(["getoption", "input:kb_layout", "-j"])
            .output()
            .await
            .context("Failed to execute hyprctl getoption")?;

        if !output.status.success() {
            // Fallback: try without -j flag and parse manually
            let output = Command::new("hyprctl")
                .args(["getoption", "input:kb_layout"])
                .output()
                .await?;

            let stdout = String::from_utf8_lossy(&output.stdout);
            // Parse output like "str: us,cz"
            for line in stdout.lines() {
                if line.starts_with("str:") {
                    let layouts_str = line.trim_start_matches("str:").trim();
                    return Ok(layouts_str
                        .split(',')
                        .map(|s| s.trim().to_string())
                        .filter(|s| !s.is_empty())
                        .collect());
                }
            }
            return Ok(Vec::new());
        }

        // Parse JSON response
        #[derive(Deserialize)]
        struct OptionResponse {
            str: String,
        }

        let response: OptionResponse = serde_json::from_slice(&output.stdout)?;
        Ok(response
            .str
            .split(',')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect())
    }

    /// Query the current active keymap from devices.
    async fn query_active_keymap(&self) -> Result<String> {
        let output = Command::new("hyprctl")
            .args(["devices", "-j"])
            .output()
            .await
            .context("Failed to execute hyprctl devices")?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            anyhow::bail!("hyprctl devices failed: {}", stderr);
        }

        let devices: HyprlandDevices =
            serde_json::from_slice(&output.stdout).context("Failed to parse hyprctl response")?;

        // Find the main keyboard, or fall back to first keyboard
        let keyboard = devices
            .keyboards
            .iter()
            .find(|k| k.main)
            .or_else(|| devices.keyboards.first())
            .ok_or_else(|| anyhow::anyhow!("No keyboard found"))?;

        Ok(keyboard.active_keymap.clone())
    }

    /// Build LayoutInfo from layout names and active keymap.
    fn build_layout_info(&self, active_keymap: &str) -> LayoutInfo {
        // If we have configured layouts, use those
        let available: Vec<String> = if !self.layout_names.is_empty() {
            self.layout_names
                .iter()
                .map(|n| extract_abbreviation(n))
                .collect()
        } else {
            // Fallback: just use the active keymap
            vec![extract_abbreviation(active_keymap)]
        };

        let current = extract_abbreviation(active_keymap);
        let current_index = available.iter().position(|a| a == &current).unwrap_or(0);

        LayoutInfo {
            current,
            available,
            current_index,
        }
    }

    /// Get the Hyprland socket path for events.
    fn get_socket_path() -> Option<String> {
        let signature = std::env::var("HYPRLAND_INSTANCE_SIGNATURE").ok()?;
        let runtime_dir = std::env::var("XDG_RUNTIME_DIR").ok()?;
        Some(format!("{}/hypr/{}/.socket2.sock", runtime_dir, signature))
    }
}

#[async_trait]
impl KeyboardLayoutBackend for HyprlandBackend {
    async fn get_layout_info(&self) -> Result<LayoutInfo> {
        let active_keymap = self.query_active_keymap().await?;
        Ok(self.build_layout_info(&active_keymap))
    }

    async fn switch_next(&self) -> Result<()> {
        let output = Command::new("hyprctl")
            .args(["switchxkblayout", "all", "next"])
            .output()
            .await
            .context("Failed to execute hyprctl switchxkblayout")?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            anyhow::bail!("hyprctl switchxkblayout failed: {}", stderr);
        }

        Ok(())
    }

    async fn switch_prev(&self) -> Result<()> {
        let output = Command::new("hyprctl")
            .args(["switchxkblayout", "all", "prev"])
            .output()
            .await
            .context("Failed to execute hyprctl switchxkblayout")?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            anyhow::bail!("hyprctl switchxkblayout failed: {}", stderr);
        }

        Ok(())
    }

    fn name(&self) -> &'static str {
        "Hyprland"
    }

    fn subscribe(&self, sender: Sender<LayoutEvent>) {
        let layout_names = self.layout_names.clone();

        tokio::spawn(async move {
            debug!("[keyboard-layout:hyprland] Starting socket subscription");

            let socket_path = match Self::get_socket_path() {
                Some(p) => p,
                None => {
                    let _ = sender.send(LayoutEvent::Error(
                        "Could not determine Hyprland socket path".to_string(),
                    ));
                    return;
                }
            };

            let stream = match UnixStream::connect(&socket_path).await {
                Ok(s) => s,
                Err(e) => {
                    let _ = sender.send(LayoutEvent::Error(format!(
                        "Failed to connect to Hyprland socket: {e}"
                    )));
                    return;
                }
            };

            let mut lines = tokio::io::BufReader::new(stream).lines();

            while let Ok(Some(line)) = lines.next_line().await {
                // Hyprland events are in format: eventname>>data
                // Layout change: activelayout>>keyboard_name,layout_name
                if let Some(data) = line.strip_prefix("activelayout>>") {
                    // Parse: keyboard_name,layout_name
                    if let Some((_keyboard, layout_name)) = data.split_once(',') {
                        debug!(
                            "[keyboard-layout:hyprland] Layout changed to: {}",
                            layout_name
                        );

                        // Build layout info
                        let available: Vec<String> = if !layout_names.is_empty() {
                            layout_names.iter().map(|n| extract_abbreviation(n)).collect()
                        } else {
                            vec![extract_abbreviation(layout_name)]
                        };

                        let current = extract_abbreviation(layout_name);
                        let current_index =
                            available.iter().position(|a| a == &current).unwrap_or(0);

                        let info = LayoutInfo {
                            current,
                            available,
                            current_index,
                        };

                        if sender.send(LayoutEvent::Changed(info)).is_err() {
                            // Receiver dropped, stop monitoring
                            break;
                        }
                    }
                }
            }

            debug!("[keyboard-layout:hyprland] Socket subscription ended");
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_hyprland_devices() {
        let json = r#"{
            "keyboards": [
                {
                    "name": "at-translated-set-2-keyboard",
                    "active_keymap": "English (US)",
                    "main": true
                }
            ]
        }"#;

        let devices: HyprlandDevices = serde_json::from_str(json).unwrap();
        assert_eq!(devices.keyboards.len(), 1);
        assert_eq!(devices.keyboards[0].active_keymap, "English (US)");
        assert!(devices.keyboards[0].main);
    }

    #[test]
    fn test_parse_hyprland_devices_multiple() {
        let json = r#"{
            "keyboards": [
                {
                    "name": "usb-keyboard",
                    "active_keymap": "German",
                    "main": false
                },
                {
                    "name": "at-translated-set-2-keyboard",
                    "active_keymap": "English (US)",
                    "main": true
                }
            ]
        }"#;

        let devices: HyprlandDevices = serde_json::from_str(json).unwrap();
        assert_eq!(devices.keyboards.len(), 2);

        // Main keyboard should be the second one
        let main_keyboard = devices.keyboards.iter().find(|k| k.main).unwrap();
        assert_eq!(main_keyboard.active_keymap, "English (US)");
    }

    #[test]
    fn test_parse_activelayout_event() {
        let line = "activelayout>>at-translated-set-2-keyboard,Czech";
        let data = line.strip_prefix("activelayout>>").unwrap();
        let (keyboard, layout) = data.split_once(',').unwrap();

        assert_eq!(keyboard, "at-translated-set-2-keyboard");
        assert_eq!(layout, "Czech");
    }
}
