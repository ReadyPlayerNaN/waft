//! Notification sound gallery entity types.

use serde::{Deserialize, Serialize};

pub const NOTIFICATION_SOUND_ENTITY_TYPE: &str = "notification-sound";

/// A sound file available in the gallery.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct NotificationSound {
    /// Filename (e.g., "alert.ogg")
    pub filename: String,
    /// Sound reference for config (e.g., "sounds/alert.ogg")
    pub reference: String,
    /// File size in bytes
    pub size: u64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn serde_roundtrip() {
        let sound = NotificationSound {
            filename: "alert.ogg".to_string(),
            reference: "sounds/alert.ogg".to_string(),
            size: 12345,
        };
        let json = serde_json::to_value(&sound).unwrap();
        let decoded: NotificationSound = serde_json::from_value(json).unwrap();
        assert_eq!(sound, decoded);
    }
}
