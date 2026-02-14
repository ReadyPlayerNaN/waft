//! Sway compositor keyboard layout backend.
//!
//! Uses `swaymsg` command for layout queries and switching.
//!
//! ## Commands
//!
//! - Query: `swaymsg -t get_inputs`
//! - Switch: `swaymsg input type:keyboard xkb_switch_layout next`
//! - Subscribe: `swaymsg -t subscribe -m '["input"]'`

use anyhow::{Context, Result};
use async_trait::async_trait;
use flume::Sender;
use log::{debug, warn};
use serde::Deserialize;
use std::process::Stdio;

use super::{KeyboardLayoutBackend, LayoutEvent, LayoutInfo, extract_abbreviation};

/// Sway input device from `swaymsg -t get_inputs`.
#[derive(Debug, Deserialize)]
#[allow(dead_code)] // xkb_active_layout_name is part of Sway's JSON but we use index instead
struct SwayInput {
    /// Input device type (e.g., "keyboard")
    #[serde(rename = "type")]
    input_type: String,
    /// XKB active layout name (e.g., "English (US)")
    xkb_active_layout_name: Option<String>,
    /// XKB layout names (e.g., ["English (US)", "German"])
    xkb_layout_names: Option<Vec<String>>,
    /// XKB active layout index
    xkb_active_layout_index: Option<usize>,
}

/// Sway input event from subscription.
#[derive(Debug, Deserialize)]
struct SwayInputEvent {
    change: String,
    input: SwayInput,
}

/// Sway compositor keyboard layout backend.
pub struct SwayBackend {
    // No state needed - all operations use command execution
}

impl SwayBackend {
    /// Create a new Sway backend.
    ///
    /// Returns `None` if the `swaymsg` command is not available.
    pub async fn new() -> Option<Self> {
        let output = std::process::Command::new("swaymsg")
            .arg("--version")
            .output()
            .ok()?;

        if output.status.success() {
            debug!("[keyboard-layout:sway] Swaymsg command available");
            Some(Self {})
        } else {
            warn!("[keyboard-layout:sway] Swaymsg command not available");
            None
        }
    }

    /// Run a swaymsg command on a background thread.
    async fn run_swaymsg(args: &[&str]) -> Result<std::process::Output> {
        let args: Vec<String> = args.iter().map(|s| s.to_string()).collect();
        let (tx, rx) = flume::bounded(1);
        std::thread::spawn(move || {
            let result = std::process::Command::new("swaymsg")
                .args(&args)
                .stdin(Stdio::null())
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .output()
                .context("Failed to execute swaymsg");
            let _ = tx.send(result);
        });
        rx.recv_async().await.context("swaymsg thread cancelled")?
    }

    /// Query keyboard info from the first keyboard device.
    async fn query_keyboard(&self) -> Result<(Vec<String>, usize)> {
        let output = Self::run_swaymsg(&["-t", "get_inputs"]).await?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            anyhow::bail!("swaymsg failed: {}", stderr);
        }

        let inputs: Vec<SwayInput> =
            serde_json::from_slice(&output.stdout).context("Failed to parse swaymsg response")?;

        Self::extract_keyboard_info(&inputs)
    }

    /// Extract keyboard layout info from input devices.
    fn extract_keyboard_info(inputs: &[SwayInput]) -> Result<(Vec<String>, usize)> {
        // Find the first keyboard with layout info
        for input in inputs {
            if input.input_type == "keyboard"
                && let (Some(names), Some(index)) =
                    (&input.xkb_layout_names, input.xkb_active_layout_index)
            {
                return Ok((names.clone(), index));
            }
        }

        anyhow::bail!("No keyboard with layout info found");
    }

    /// Convert layout names and index to LayoutInfo.
    fn to_layout_info(names: &[String], current_index: usize) -> LayoutInfo {
        let available: Vec<String> = names.iter().map(|n| extract_abbreviation(n)).collect();

        let current_index = current_index.min(available.len().saturating_sub(1));
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
impl KeyboardLayoutBackend for SwayBackend {
    async fn get_layout_info(&self) -> Result<LayoutInfo> {
        let (names, current_index) = self.query_keyboard().await?;

        if names.is_empty() {
            anyhow::bail!("No keyboard layouts configured in Sway");
        }

        Ok(Self::to_layout_info(&names, current_index))
    }

    async fn switch_next(&self) -> Result<()> {
        let output =
            Self::run_swaymsg(&["input", "type:keyboard", "xkb_switch_layout", "next"]).await?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            anyhow::bail!("swaymsg xkb_switch_layout failed: {}", stderr);
        }

        Ok(())
    }

    async fn switch_prev(&self) -> Result<()> {
        let output =
            Self::run_swaymsg(&["input", "type:keyboard", "xkb_switch_layout", "prev"]).await?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            anyhow::bail!("swaymsg xkb_switch_layout failed: {}", stderr);
        }

        Ok(())
    }

    fn name(&self) -> &'static str {
        "Sway"
    }

    fn subscribe(&self, sender: Sender<LayoutEvent>) {
        // Use std::thread + std::process to avoid needing tokio's IO reactor
        std::thread::spawn(move || {
            use std::io::BufRead;

            debug!("[keyboard-layout:sway] Starting input event subscription");

            let mut child = match std::process::Command::new("swaymsg")
                .args(["-t", "subscribe", "-m", "[\"input\"]"])
                .stdin(Stdio::null())
                .stdout(Stdio::piped())
                .stderr(Stdio::null())
                .spawn()
            {
                Ok(c) => c,
                Err(e) => {
                    let _ = sender.send(LayoutEvent::Error(format!(
                        "Failed to spawn swaymsg subscribe: {e}"
                    )));
                    return;
                }
            };

            let stdout = match child.stdout.take() {
                Some(s) => s,
                None => {
                    let _ = sender.send(LayoutEvent::Error(
                        "Failed to capture swaymsg stdout".to_string(),
                    ));
                    return;
                }
            };

            let reader = std::io::BufReader::new(stdout);

            for line in reader.lines() {
                let line = match line {
                    Ok(l) => l,
                    Err(_) => break,
                };
                let line = line.trim();
                if line.is_empty() {
                    continue;
                }

                match serde_json::from_str::<SwayInputEvent>(line) {
                    Ok(event) => {
                        if event.change == "xkb_layout"
                            && event.input.input_type == "keyboard"
                            && let (Some(names), Some(index)) = (
                                &event.input.xkb_layout_names,
                                event.input.xkb_active_layout_index,
                            )
                        {
                            debug!("[keyboard-layout:sway] Layout changed to index {}", index);
                            let info = Self::to_layout_info(names, index);
                            if sender.send(LayoutEvent::Changed(info)).is_err() {
                                break;
                            }
                        }
                    }
                    Err(e) => {
                        debug!("[keyboard-layout:sway] Failed to parse event: {e}");
                    }
                }
            }

            debug!("[keyboard-layout:sway] Input subscription ended");
            let _ = child.kill();
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_sway_input() {
        let json = r#"[
            {
                "type": "keyboard",
                "xkb_active_layout_name": "English (US)",
                "xkb_layout_names": ["English (US)", "German"],
                "xkb_active_layout_index": 0
            }
        ]"#;

        let inputs: Vec<SwayInput> = serde_json::from_str(json).unwrap();
        assert_eq!(inputs.len(), 1);
        assert_eq!(inputs[0].input_type, "keyboard");
        assert_eq!(
            inputs[0].xkb_active_layout_name,
            Some("English (US)".to_string())
        );
        assert_eq!(
            inputs[0].xkb_layout_names,
            Some(vec!["English (US)".to_string(), "German".to_string()])
        );
        assert_eq!(inputs[0].xkb_active_layout_index, Some(0));
    }

    #[test]
    fn test_parse_sway_input_multiple_devices() {
        let json = r#"[
            {
                "type": "pointer",
                "xkb_active_layout_name": null,
                "xkb_layout_names": null,
                "xkb_active_layout_index": null
            },
            {
                "type": "keyboard",
                "xkb_active_layout_name": "Czech",
                "xkb_layout_names": ["English (US)", "Czech"],
                "xkb_active_layout_index": 1
            }
        ]"#;

        let inputs: Vec<SwayInput> = serde_json::from_str(json).unwrap();
        assert_eq!(inputs.len(), 2);

        // First device is a pointer
        assert_eq!(inputs[0].input_type, "pointer");

        // Second device is a keyboard
        assert_eq!(inputs[1].input_type, "keyboard");
        assert_eq!(inputs[1].xkb_active_layout_index, Some(1));
    }

    #[test]
    fn test_parse_sway_input_event() {
        let json = r#"{"change":"xkb_layout","input":{"type":"keyboard","xkb_active_layout_name":"Czech","xkb_layout_names":["English (US)","Czech"],"xkb_active_layout_index":1}}"#;
        let event: SwayInputEvent = serde_json::from_str(json).unwrap();

        assert_eq!(event.change, "xkb_layout");
        assert_eq!(event.input.input_type, "keyboard");
        assert_eq!(event.input.xkb_active_layout_index, Some(1));
    }
}
