use serde::{Deserialize, Serialize};

/// Entity type identifier for session state.
pub const SESSION_ENTITY_TYPE: &str = "session";

/// Entity type identifier for sleep inhibitors.
pub const SLEEP_INHIBITOR_ENTITY_TYPE: &str = "sleep-inhibitor";

/// User session information.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Session {
    pub user_name: Option<String>,
    pub screen_name: Option<String>,
}

/// A sleep/screensaver inhibitor (caffeine mode).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct SleepInhibitor {
    pub active: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn session_serde_roundtrip() {
        let session = Session {
            user_name: Some("alice".to_string()),
            screen_name: Some("Alice Smith".to_string()),
        };
        let json = serde_json::to_value(&session).unwrap();
        let decoded: Session = serde_json::from_value(json).unwrap();
        assert_eq!(session, decoded);
    }

    #[test]
    fn session_serde_roundtrip_empty() {
        let session = Session {
            user_name: None,
            screen_name: None,
        };
        let json = serde_json::to_value(&session).unwrap();
        let decoded: Session = serde_json::from_value(json).unwrap();
        assert_eq!(session, decoded);
    }

    #[test]
    fn sleep_inhibitor_serde_roundtrip() {
        let inhibitor = SleepInhibitor { active: true };
        let json = serde_json::to_value(inhibitor).unwrap();
        let decoded: SleepInhibitor = serde_json::from_value(json).unwrap();
        assert_eq!(inhibitor, decoded);
    }
}
