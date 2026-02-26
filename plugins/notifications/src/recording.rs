//! Notification recording module for debugging.
//!
//! Writes serialized notification data as JSON Lines to a log file
//! at `$XDG_RUNTIME_DIR/waft/notifications-recording.jsonl`.

use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::PathBuf;
use std::sync::{Arc, Mutex as StdMutex};
use std::time::{SystemTime, UNIX_EPOCH};

use log::warn;
use waft_plugin::StateLocker;
use waft_protocol::entity::notification as proto;

/// Notification recorder that appends JSON Lines to a log file.
pub struct NotificationRecorder {
    active: Arc<StdMutex<bool>>,
    log_path: PathBuf,
}

impl NotificationRecorder {
    /// Create a new recorder.
    ///
    /// Resolves the log path from `$XDG_RUNTIME_DIR/waft/notifications-recording.jsonl`.
    /// Creates the `waft/` subdirectory if needed.
    pub fn new(active: bool) -> Self {
        let runtime_dir = std::env::var("XDG_RUNTIME_DIR")
            .unwrap_or_else(|_| "/tmp".to_string());
        let waft_dir = PathBuf::from(&runtime_dir).join("waft");

        if let Err(e) = fs::create_dir_all(&waft_dir) {
            warn!("[notifications/recording] failed to create waft dir: {e}");
        }

        let log_path = waft_dir.join("notifications-recording.jsonl");

        Self {
            active: Arc::new(StdMutex::new(active)),
            log_path,
        }
    }

    /// Record a notification to the log file.
    ///
    /// If recording is not active, this is a no-op.
    /// File I/O errors are logged but never propagated -- recording must not
    /// disrupt the notification pipeline.
    pub fn record(&self, notification: &proto::Notification, urn: &str) {
        let active = *self.active.lock_or_recover();

        if !active {
            return;
        }

        let recorded_at_ms = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_millis() as i64)
            .unwrap_or(0);

        // Build a sanitized copy with byte icons replaced by placeholders
        let sanitized = sanitize_notification(notification);

        // Build the record as a JSON object with extra fields
        let mut record = match serde_json::to_value(&sanitized) {
            Ok(serde_json::Value::Object(map)) => map,
            Ok(_) => {
                warn!("[notifications/recording] notification serialized to non-object");
                return;
            }
            Err(e) => {
                warn!("[notifications/recording] failed to serialize notification: {e}");
                return;
            }
        };

        record.insert("urn".to_string(), serde_json::Value::String(urn.to_string()));
        record.insert(
            "recorded_at_ms".to_string(),
            serde_json::Value::Number(serde_json::Number::from(recorded_at_ms)),
        );

        let line = match serde_json::to_string(&record) {
            Ok(s) => s,
            Err(e) => {
                warn!("[notifications/recording] failed to serialize record: {e}");
                return;
            }
        };

        if let Err(e) = self.append_line(&line) {
            warn!("[notifications/recording] failed to write record: {e}");
        }
    }

    /// Set the active state.
    ///
    /// When transitioning from inactive to active, truncates the log file
    /// to start a clean recording session.
    pub fn set_active(&self, new_active: bool) {
        let mut guard = self.active.lock_or_recover();

        let was_active = *guard;
        *guard = new_active;

        // Truncate log file when transitioning from inactive to active
        if !was_active && new_active
            && let Err(e) = fs::write(&self.log_path, b"") {
                warn!("[notifications/recording] failed to truncate log file: {e}");
            }
    }

    /// Returns whether recording is currently active.
    pub fn is_active(&self) -> bool {
        *self.active.lock_or_recover()
    }

    /// Append a single line to the log file.
    fn append_line(&self, line: &str) -> std::io::Result<()> {
        let mut file = OpenOptions::new()
            .append(true)
            .create(true)
            .open(&self.log_path)?;
        writeln!(file, "{line}")
    }
}

/// Replace `NotificationIconHint::Bytes` entries with a placeholder string
/// to avoid bloating the log with raw image data.
fn sanitize_notification(notification: &proto::Notification) -> proto::Notification {
    let icon_hints = notification
        .icon_hints
        .iter()
        .map(|hint| match hint {
            proto::NotificationIconHint::Bytes(data) => {
                proto::NotificationIconHint::FilePath(format!("<bytes:{}>", data.len()))
            }
            other => other.clone(),
        })
        .collect();

    proto::Notification {
        icon_hints,
        ..notification.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_test_notification() -> proto::Notification {
        proto::Notification {
            title: "Test Title".to_string(),
            description: "Test body".to_string(),
            app_name: Some("TestApp".to_string()),
            app_id: Some("test-app".to_string()),
            urgency: proto::NotificationUrgency::Normal,
            actions: vec![proto::NotificationAction {
                key: "default".to_string(),
                label: "Open".to_string(),
            }],
            icon_hints: vec![
                proto::NotificationIconHint::Themed("dialog-information".to_string()),
                proto::NotificationIconHint::Bytes(vec![0x89, 0x50, 0x4E, 0x47]),
            ],
            created_at_ms: 1707753600000,
            resident: false,
            workspace: None,
            suppress_toast: false,
            ttl: None,
        }
    }

    #[test]
    fn sanitize_replaces_bytes_with_placeholder() {
        let notif = make_test_notification();
        let sanitized = sanitize_notification(&notif);

        assert_eq!(sanitized.icon_hints.len(), 2);
        assert_eq!(
            sanitized.icon_hints[0],
            proto::NotificationIconHint::Themed("dialog-information".to_string())
        );
        assert_eq!(
            sanitized.icon_hints[1],
            proto::NotificationIconHint::FilePath("<bytes:4>".to_string())
        );
    }

    #[test]
    fn recorder_inactive_does_not_write() {
        let dir = tempfile::tempdir().unwrap();
        let log_path = dir.path().join("test.jsonl");

        let recorder = NotificationRecorder {
            active: Arc::new(StdMutex::new(false)),
            log_path: log_path.clone(),
        };

        let notif = make_test_notification();
        recorder.record(&notif, "notifications/notification/1");

        assert!(!log_path.exists());
    }

    #[test]
    fn recorder_active_writes_jsonl() {
        let dir = tempfile::tempdir().unwrap();
        let log_path = dir.path().join("test.jsonl");

        let recorder = NotificationRecorder {
            active: Arc::new(StdMutex::new(true)),
            log_path: log_path.clone(),
        };

        let notif = make_test_notification();
        recorder.record(&notif, "notifications/notification/42");

        let content = fs::read_to_string(&log_path).unwrap();
        let lines: Vec<&str> = content.lines().collect();
        assert_eq!(lines.len(), 1);

        let record: serde_json::Value = serde_json::from_str(lines[0]).unwrap();
        assert_eq!(record["urn"], "notifications/notification/42");
        assert_eq!(record["title"], "Test Title");
        assert!(record["recorded_at_ms"].is_number());
        // Bytes icon should be replaced
        let icons = record["icon_hints"].as_array().unwrap();
        assert_eq!(icons[1]["FilePath"], "<bytes:4>");
    }

    #[test]
    fn set_active_truncates_on_enable() {
        let dir = tempfile::tempdir().unwrap();
        let log_path = dir.path().join("test.jsonl");

        let recorder = NotificationRecorder {
            active: Arc::new(StdMutex::new(true)),
            log_path: log_path.clone(),
        };

        let notif = make_test_notification();
        recorder.record(&notif, "notifications/notification/1");
        assert!(fs::read_to_string(&log_path).unwrap().len() > 0);

        // Disable
        recorder.set_active(false);
        assert!(!recorder.is_active());

        // Re-enable should truncate
        recorder.set_active(true);
        assert!(recorder.is_active());
        let content = fs::read_to_string(&log_path).unwrap();
        assert!(content.is_empty());
    }

    #[test]
    fn set_active_no_truncate_when_already_active() {
        let dir = tempfile::tempdir().unwrap();
        let log_path = dir.path().join("test.jsonl");

        let recorder = NotificationRecorder {
            active: Arc::new(StdMutex::new(true)),
            log_path: log_path.clone(),
        };

        let notif = make_test_notification();
        recorder.record(&notif, "notifications/notification/1");
        let original_content = fs::read_to_string(&log_path).unwrap();

        // set_active(true) when already active should NOT truncate
        recorder.set_active(true);
        let content = fs::read_to_string(&log_path).unwrap();
        assert_eq!(content, original_content);
    }
}
