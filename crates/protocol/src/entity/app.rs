use serde::{Deserialize, Serialize};

/// Entity type identifier for launchable applications.
pub const ENTITY_TYPE: &str = "app";

/// A launchable application (e.g. waft-settings).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct App {
    pub name: String,
    pub icon: String,
    pub available: bool,
    #[serde(default)]
    pub keywords: Vec<String>,
    #[serde(default)]
    pub description: Option<String>,
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
            keywords: vec![],
            description: None,
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
            keywords: vec![],
            description: None,
        };
        let json = serde_json::to_value(&app).unwrap();
        let decoded: App = serde_json::from_value(json).unwrap();
        assert_eq!(app, decoded);
    }

    #[test]
    fn app_serde_roundtrip_with_keywords_and_description() {
        let app = App {
            name: "Firefox".to_string(),
            icon: "firefox".to_string(),
            available: true,
            keywords: vec!["browser".to_string(), "web".to_string()],
            description: Some("Web browser".to_string()),
        };
        let json = serde_json::to_value(&app).unwrap();
        let decoded: App = serde_json::from_value(json).unwrap();
        assert_eq!(app, decoded);
    }

    #[test]
    fn app_serde_backward_compat_missing_fields() {
        // Old JSON without keywords/description must deserialize with defaults
        let json = serde_json::json!({
            "name": "Settings",
            "icon": "preferences-system-symbolic",
            "available": true
        });
        let app: App = serde_json::from_value(json).unwrap();
        assert_eq!(app.keywords, Vec::<String>::new());
        assert_eq!(app.description, None);
    }
}
