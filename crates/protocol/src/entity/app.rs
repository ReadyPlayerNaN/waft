use serde::{Deserialize, Serialize};

/// Entity type identifier for launchable applications.
pub const ENTITY_TYPE: &str = "app";

/// A launchable application (e.g. waft-settings).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct App {
    pub name: String,
    pub icon: String,
    pub available: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn app_serde_roundtrip_available() {
        let app = App {
            name: "Settings".to_string(),
            icon: "preferences-system-symbolic".to_string(),
            available: true,
        };
        let json = serde_json::to_value(&app).unwrap();
        let decoded: App = serde_json::from_value(json).unwrap();
        assert_eq!(app, decoded);
    }

    #[test]
    fn app_serde_roundtrip_unavailable() {
        let app = App {
            name: "Settings".to_string(),
            icon: "preferences-system-symbolic".to_string(),
            available: false,
        };
        let json = serde_json::to_value(&app).unwrap();
        let decoded: App = serde_json::from_value(json).unwrap();
        assert_eq!(app, decoded);
    }
}
