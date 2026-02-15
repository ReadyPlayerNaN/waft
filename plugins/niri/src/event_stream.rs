//! Niri event stream monitoring.
//!
//! Spawns a `niri msg --json event-stream` process on a background thread
//! and sends parsed events via a flume channel.

use serde::Deserialize;
use std::process::Stdio;

/// Events extracted from the Niri event stream.
#[derive(Debug)]
pub enum NiriEvent {
    /// Full keyboard layout info (sent at startup and on config reload).
    KeyboardLayoutsChanged {
        names: Vec<String>,
        current_idx: usize,
    },
    /// Layout switch event (just the index).
    KeyboardLayoutSwitched { idx: usize },
    /// Config reloaded -- re-query outputs since display config may have changed.
    ConfigReloaded,
}

/// Raw event from `niri msg --json event-stream`.
#[derive(Debug, Deserialize)]
struct RawNiriEvent {
    #[serde(rename = "KeyboardLayoutsChanged")]
    keyboard_layouts_changed: Option<KeyboardLayoutsChangedPayload>,
    #[serde(rename = "KeyboardLayoutSwitched")]
    keyboard_layout_switched: Option<KeyboardLayoutSwitchedPayload>,
    #[serde(rename = "ConfigLoaded")]
    config_loaded: Option<serde_json::Value>,
}

#[derive(Debug, Deserialize)]
struct KeyboardLayoutsChangedPayload {
    keyboard_layouts: LayoutsInfo,
}

#[derive(Debug, Deserialize)]
struct LayoutsInfo {
    names: Vec<String>,
    current_idx: usize,
}

#[derive(Debug, Deserialize)]
struct KeyboardLayoutSwitchedPayload {
    idx: usize,
}

/// Spawn the event stream monitoring thread.
///
/// Runs `niri msg --json event-stream` in a background thread and sends
/// parsed events through the returned flume receiver. The thread runs until
/// the process exits or the receiver is dropped.
pub fn spawn_event_stream() -> flume::Receiver<NiriEvent> {
    let (tx, rx) = flume::unbounded();

    std::thread::spawn(move || {
        use std::io::BufRead;

        log::info!("[niri] Starting event stream");

        let mut child = match std::process::Command::new("niri")
            .args(["msg", "--json", "event-stream"])
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
        {
            Ok(c) => c,
            Err(e) => {
                log::error!("[niri] Failed to spawn event stream: {e}");
                return;
            }
        };

        let stdout = match child.stdout.take() {
            Some(s) => s,
            None => {
                log::error!("[niri] Failed to capture event stream stdout");
                return;
            }
        };

        let reader = std::io::BufReader::new(stdout);

        for line in reader.lines() {
            let line = match line {
                Ok(l) => l,
                Err(e) => {
                    log::error!("[niri] Error reading event stream: {e}");
                    break;
                }
            };

            let line = line.trim();
            if line.is_empty() {
                continue;
            }

            let raw: RawNiriEvent = match serde_json::from_str(line) {
                Ok(e) => e,
                Err(_) => {
                    // Unknown event type, skip silently (many events we don't care about)
                    continue;
                }
            };

            if let Some(payload) = raw.keyboard_layouts_changed {
                log::debug!(
                    "[niri] KeyboardLayoutsChanged: {} layouts, index {}",
                    payload.keyboard_layouts.names.len(),
                    payload.keyboard_layouts.current_idx
                );
                if tx
                    .send(NiriEvent::KeyboardLayoutsChanged {
                        names: payload.keyboard_layouts.names,
                        current_idx: payload.keyboard_layouts.current_idx,
                    })
                    .is_err()
                {
                    break;
                }
            } else if let Some(payload) = raw.keyboard_layout_switched {
                log::debug!("[niri] KeyboardLayoutSwitched: index {}", payload.idx);
                if tx
                    .send(NiriEvent::KeyboardLayoutSwitched { idx: payload.idx })
                    .is_err()
                {
                    break;
                }
            } else if raw.config_loaded.is_some() {
                log::debug!("[niri] ConfigLoaded: re-querying outputs");
                if tx.send(NiriEvent::ConfigReloaded).is_err() {
                    break;
                }
            }
        }

        log::warn!("[niri] Event stream loop ended");
        let _ = child.kill();
        let _ = child.wait();
    });

    rx
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_keyboard_layouts_changed() {
        let json = r#"{"KeyboardLayoutsChanged":{"keyboard_layouts":{"names":["English (US)","Czech"],"current_idx":1}}}"#;
        let event: RawNiriEvent = serde_json::from_str(json).unwrap();

        let payload = event
            .keyboard_layouts_changed
            .expect("Expected KeyboardLayoutsChanged");
        assert_eq!(payload.keyboard_layouts.current_idx, 1);
        assert_eq!(payload.keyboard_layouts.names.len(), 2);
    }

    #[test]
    fn test_parse_keyboard_layout_switched() {
        let json = r#"{"KeyboardLayoutSwitched":{"idx":1}}"#;
        let event: RawNiriEvent = serde_json::from_str(json).unwrap();

        let payload = event
            .keyboard_layout_switched
            .expect("Expected KeyboardLayoutSwitched");
        assert_eq!(payload.idx, 1);
    }

    #[test]
    fn test_parse_config_loaded() {
        let json = r#"{"ConfigLoaded":{}}"#;
        let event: RawNiriEvent = serde_json::from_str(json).unwrap();
        assert!(event.config_loaded.is_some());
    }

    #[test]
    fn test_parse_unknown_event() {
        let json = r#"{"WindowOpenedOrChanged":{"window":{}}}"#;
        let event: RawNiriEvent = serde_json::from_str(json).unwrap();
        assert!(event.keyboard_layouts_changed.is_none());
        assert!(event.keyboard_layout_switched.is_none());
        assert!(event.config_loaded.is_none());
    }
}
