use serde::{Deserialize, Serialize};

/// Entity type identifier for notifications.
pub const NOTIFICATION_ENTITY_TYPE: &str = "notification";

/// Entity type identifier for Do Not Disturb state.
pub const DND_ENTITY_TYPE: &str = "dnd";

/// A desktop notification.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Notification {
    pub title: String,
    pub description: String,
    pub app_name: Option<String>,
    pub app_id: Option<String>,
    pub urgency: NotificationUrgency,
    pub actions: Vec<NotificationAction>,
    pub icon_hints: Vec<NotificationIconHint>,
    pub created_at_ms: i64,
    pub resident: bool,
}

/// Notification urgency level per the freedesktop specification.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum NotificationUrgency {
    Low,
    Normal,
    Critical,
}

/// An action that can be invoked on a notification.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NotificationAction {
    pub key: String,
    pub label: String,
}

/// Icon hint for a notification, in priority order.
///
/// Apps typically provide multiple hints; consumers try them in order.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum NotificationIconHint {
    Bytes(Vec<u8>),
    FilePath(String),
    Themed(String),
}

/// Do Not Disturb state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Dnd {
    pub active: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn notification_serde_roundtrip() {
        let notification = Notification {
            title: "New Message".to_string(),
            description: "You have a new message from Alice".to_string(),
            app_name: Some("Telegram".to_string()),
            app_id: Some("telegram".to_string()),
            urgency: NotificationUrgency::Normal,
            actions: vec![
                NotificationAction {
                    key: "default".to_string(),
                    label: "Open".to_string(),
                },
                NotificationAction {
                    key: "reply".to_string(),
                    label: "Reply".to_string(),
                },
            ],
            icon_hints: vec![
                NotificationIconHint::Themed("telegram".to_string()),
                NotificationIconHint::FilePath("/usr/share/icons/telegram.png".to_string()),
            ],
            created_at_ms: 1707753600000,
            resident: false,
        };
        let json = serde_json::to_value(&notification).unwrap();
        let decoded: Notification = serde_json::from_value(json).unwrap();
        assert_eq!(notification, decoded);
    }

    #[test]
    fn notification_with_bytes_icon() {
        let notification = Notification {
            title: "Test".to_string(),
            description: "Body".to_string(),
            app_name: None,
            app_id: None,
            urgency: NotificationUrgency::Low,
            actions: vec![],
            icon_hints: vec![NotificationIconHint::Bytes(vec![0x89, 0x50, 0x4E, 0x47])],
            created_at_ms: 1707753600000,
            resident: true,
        };
        let json = serde_json::to_value(&notification).unwrap();
        let decoded: Notification = serde_json::from_value(json).unwrap();
        assert_eq!(notification, decoded);
    }

    #[test]
    fn dnd_serde_roundtrip() {
        let dnd = Dnd { active: true };
        let json = serde_json::to_value(dnd).unwrap();
        let decoded: Dnd = serde_json::from_value(json).unwrap();
        assert_eq!(dnd, decoded);
    }
}
