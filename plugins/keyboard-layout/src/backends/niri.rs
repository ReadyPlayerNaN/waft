//! Niri compositor keyboard layout backend.
//!
//! Uses `niri msg` command for layout queries and switching.
//!
//! ## Commands
//!
//! - Query: `niri msg --json keyboard-layouts`
//! - Switch next: `niri msg action switch-layout next`
//! - Switch prev: `niri msg action switch-layout prev`
//! - Event stream: `niri msg --json event-stream`

use anyhow::{Context, Result};
use async_trait::async_trait;
use flume::Sender;
use log::{debug, warn};
use serde::Deserialize;
use std::process::Stdio;

use super::{KeyboardLayoutBackend, LayoutEvent, LayoutInfo, extract_abbreviation};

/// Response from `niri msg --json keyboard-layouts`.
#[derive(Debug, Deserialize)]
struct NiriLayoutsResponse {
    /// Layout names (e.g., ["English (US)", "Czech (QWERTY)"])
    names: Vec<String>,
    /// Index of the currently active layout
    current_idx: usize,
}

/// Event from `niri msg --json event-stream`.
/// Events are in format: {"EventName": {"field": value}}
#[derive(Debug, Deserialize)]
struct NiriEvent {
    /// Full keyboard layouts info (sent once at startup)
    #[serde(rename = "KeyboardLayoutsChanged")]
    keyboard_layouts_changed: Option<KeyboardLayoutsChangedPayload>,
    /// Layout switch event (sent when user switches layout, contains only index)
    #[serde(rename = "KeyboardLayoutSwitched")]
    keyboard_layout_switched: Option<KeyboardLayoutSwitchedPayload>,
}

/// Payload for KeyboardLayoutsChanged event (full info).
#[derive(Debug, Deserialize)]
struct KeyboardLayoutsChangedPayload {
    keyboard_layouts: NiriLayoutsResponse,
}

/// Payload for KeyboardLayoutSwitched event (just the index).
#[derive(Debug, Deserialize)]
struct KeyboardLayoutSwitchedPayload {
    idx: usize,
}

/// Niri compositor keyboard layout backend.
pub struct NiriBackend {
    // No state needed - all operations use command execution
}

impl NiriBackend {
    /// Create a new Niri backend.
    ///
    /// Returns `None` if the `niri` command is not available.
    pub async fn new() -> Option<Self> {
        // Verify niri command is available
        let output = std::process::Command::new("niri")
            .arg("--version")
            .output()
            .ok()?;

        if output.status.success() {
            debug!("[keyboard-layout:niri] Niri command available");
            Some(Self {})
        } else {
            warn!("[keyboard-layout:niri] Niri command not available");
            None
        }
    }

    /// Query keyboard layouts from Niri using std::process on a background thread.
    async fn query_layouts(&self) -> Result<NiriLayoutsResponse> {
        let (tx, rx) = flume::bounded(1);
        std::thread::spawn(move || {
            let result = std::process::Command::new("niri")
                .args(["msg", "--json", "keyboard-layouts"])
                .stdin(Stdio::null())
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .output()
                .context("Failed to execute niri msg");
            let _ = tx.send(result);
        });

        let output = rx
            .recv_async()
            .await
            .context("niri command thread cancelled")?
            .context("Failed to execute niri msg")?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            anyhow::bail!("niri msg failed: {}", stderr);
        }

        let response: NiriLayoutsResponse =
            serde_json::from_slice(&output.stdout).context("Failed to parse niri response")?;

        Ok(response)
    }

    /// Execute a layout switch action using std::process on a background thread.
    async fn switch_layout(&self, direction: &str) -> Result<()> {
        let direction = direction.to_string();
        let (tx, rx) = flume::bounded(1);
        std::thread::spawn(move || {
            let result = std::process::Command::new("niri")
                .args(["msg", "action", "switch-layout", &direction])
                .stdin(Stdio::null())
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .output()
                .context("Failed to execute niri msg");
            let _ = tx.send(result);
        });

        let output = rx
            .recv_async()
            .await
            .context("niri command thread cancelled")?
            .context("Failed to execute niri msg")?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            anyhow::bail!("niri switch-layout failed: {}", stderr);
        }

        Ok(())
    }

    /// Convert NiriLayoutsResponse to LayoutInfo
    fn response_to_layout_info(response: &NiriLayoutsResponse) -> LayoutInfo {
        let available: Vec<String> = response
            .names
            .iter()
            .map(|n| extract_abbreviation(n))
            .collect();

        let current_index = response.current_idx.min(available.len().saturating_sub(1));
        let current = available
            .get(current_index)
            .cloned()
            .unwrap_or_else(|| "??".to_string());

        LayoutInfo {
            current,
            available,
            current_index,
        }
    }
}

#[async_trait]
impl KeyboardLayoutBackend for NiriBackend {
    async fn get_layout_info(&self) -> Result<LayoutInfo> {
        let response = self.query_layouts().await?;

        if response.names.is_empty() {
            anyhow::bail!("No keyboard layouts configured in Niri");
        }

        Ok(Self::response_to_layout_info(&response))
    }

    async fn switch_next(&self) -> Result<()> {
        self.switch_layout("next").await
    }

    async fn switch_prev(&self) -> Result<()> {
        self.switch_layout("prev").await
    }

    fn name(&self) -> &'static str {
        "Niri"
    }

    fn subscribe(&self, sender: Sender<LayoutEvent>) {
        log::info!("[keyboard-layout:niri] Setting up event stream subscription");

        // Use std::thread + std::process to avoid needing tokio's IO reactor
        std::thread::spawn(move || {
            use std::io::BufRead;

            log::info!("[keyboard-layout:niri] Thread started, spawning niri event-stream");

            let mut child = match std::process::Command::new("niri")
                .args(["msg", "--json", "event-stream"])
                .stdin(Stdio::null())
                .stdout(Stdio::piped())
                .stderr(Stdio::null())
                .spawn()
            {
                Ok(c) => c,
                Err(e) => {
                    let _ = sender.send(LayoutEvent::Error(format!(
                        "Failed to spawn niri event-stream: {e}"
                    )));
                    return;
                }
            };

            let stdout = match child.stdout.take() {
                Some(s) => s,
                None => {
                    let _ = sender.send(LayoutEvent::Error(
                        "Failed to capture niri stdout".to_string(),
                    ));
                    return;
                }
            };

            let reader = std::io::BufReader::new(stdout);

            // Cache layout names from the initial KeyboardLayoutsChanged event
            let mut cached_layouts: Vec<String> = Vec::new();

            log::info!("[keyboard-layout:niri] Starting to read lines from event-stream");
            for line in reader.lines() {
                let line = match line {
                    Ok(l) => l,
                    Err(e) => {
                        log::error!("[keyboard-layout:niri] Error reading from event stream: {e}");
                        break;
                    }
                };
                let line = line.trim();
                if line.is_empty() {
                    continue;
                }

                match serde_json::from_str::<NiriEvent>(line) {
                    Ok(event) => {
                        // Handle full layout info (sent at startup)
                        if let Some(payload) = event.keyboard_layouts_changed {
                            log::info!(
                                "[keyboard-layout:niri] Received full layout info, index {}",
                                payload.keyboard_layouts.current_idx
                            );
                            cached_layouts = payload.keyboard_layouts.names.clone();
                            let info = Self::response_to_layout_info(&payload.keyboard_layouts);
                            if sender.send(LayoutEvent::Changed(info)).is_err() {
                                log::warn!(
                                    "[keyboard-layout:niri] Failed to send event, receiver dropped"
                                );
                                break;
                            }
                        }
                        // Handle layout switch (just index, use cached names)
                        else if let Some(payload) = event.keyboard_layout_switched {
                            log::info!(
                                "[keyboard-layout:niri] Layout switched to index {}",
                                payload.idx
                            );
                            if !cached_layouts.is_empty() {
                                let available: Vec<String> = cached_layouts
                                    .iter()
                                    .map(|n| extract_abbreviation(n))
                                    .collect();
                                let current_index =
                                    payload.idx.min(available.len().saturating_sub(1));
                                let current = available
                                    .get(current_index)
                                    .cloned()
                                    .unwrap_or_else(|| "??".to_string());

                                let info = LayoutInfo {
                                    current,
                                    available,
                                    current_index,
                                };
                                if sender.send(LayoutEvent::Changed(info)).is_err() {
                                    log::warn!(
                                        "[keyboard-layout:niri] Failed to send event, receiver dropped"
                                    );
                                    break;
                                }
                            } else {
                                log::warn!(
                                    "[keyboard-layout:niri] Got switch event but no cached layouts"
                                );
                            }
                        }
                    }
                    Err(e) => {
                        log::debug!("[keyboard-layout:niri] Failed to parse event: {e}");
                    }
                }
            }

            log::warn!("[keyboard-layout:niri] Event stream loop ended");
            let _ = child.kill();
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_niri_response() {
        let json = r#"{"names":["English (US)","Czech (QWERTY)"],"current_idx":0}"#;
        let response: NiriLayoutsResponse = serde_json::from_str(json).unwrap();

        assert_eq!(response.names.len(), 2);
        assert_eq!(response.names[0], "English (US)");
        assert_eq!(response.names[1], "Czech (QWERTY)");
        assert_eq!(response.current_idx, 0);
    }

    #[test]
    fn test_parse_niri_response_single_layout() {
        let json = r#"{"names":["English (US)"],"current_idx":0}"#;
        let response: NiriLayoutsResponse = serde_json::from_str(json).unwrap();

        assert_eq!(response.names.len(), 1);
        assert_eq!(response.current_idx, 0);
    }

    #[test]
    fn test_parse_niri_response_second_active() {
        let json = r#"{"names":["English (US)","German"],"current_idx":1}"#;
        let response: NiriLayoutsResponse = serde_json::from_str(json).unwrap();

        assert_eq!(response.current_idx, 1);
    }

    #[test]
    fn test_parse_niri_event_keyboard_layouts_changed() {
        let json = r#"{"KeyboardLayoutsChanged":{"keyboard_layouts":{"names":["English (US)","Czech"],"current_idx":1}}}"#;
        let event: NiriEvent = serde_json::from_str(json).unwrap();

        let payload = event
            .keyboard_layouts_changed
            .expect("Expected KeyboardLayoutsChanged");
        assert_eq!(payload.keyboard_layouts.current_idx, 1);
        assert_eq!(payload.keyboard_layouts.names.len(), 2);
    }

    #[test]
    fn test_parse_niri_event_layout_switched() {
        let json = r#"{"KeyboardLayoutSwitched":{"idx":1}}"#;
        let event: NiriEvent = serde_json::from_str(json).unwrap();

        let payload = event
            .keyboard_layout_switched
            .expect("Expected KeyboardLayoutSwitched");
        assert_eq!(payload.idx, 1);
        assert!(event.keyboard_layouts_changed.is_none());
    }

    #[test]
    fn test_parse_niri_event_other() {
        let json = r#"{"WindowOpenedOrChanged":{"window":{}}}"#;
        let event: NiriEvent = serde_json::from_str(json).unwrap();

        assert!(event.keyboard_layouts_changed.is_none());
        assert!(event.keyboard_layout_switched.is_none());
    }
}
