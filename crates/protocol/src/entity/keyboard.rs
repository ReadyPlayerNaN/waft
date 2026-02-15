use serde::{Deserialize, Serialize};

/// Entity type identifier for keyboard layouts.
pub const ENTITY_TYPE: &str = "keyboard-layout";

/// Entity type identifier for keyboard layout configuration.
pub const CONFIG_ENTITY_TYPE: &str = "keyboard-layout-config";

/// Active keyboard layout and available alternatives.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct KeyboardLayout {
    pub current: String,
    pub available: Vec<String>,
}

/// Keyboard layout configuration entity.
/// Represents the configured layouts in compositor's config file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct KeyboardLayoutConfig {
    /// Configuration mode: "editable", "external-file", "system-default", "error"
    pub mode: String,
    /// Configured layout codes (e.g., ["us", "de", "cz"])
    pub layouts: Vec<String>,
    /// XKB variant (e.g., "dvorak")
    pub variant: Option<String>,
    /// XKB options (e.g., "grp:win_space_toggle,compose:ralt")
    pub options: Option<String>,
    /// Path to external keymap file (set if mode == "external-file")
    pub file_path: Option<String>,
    /// Error message (set if mode == "error")
    pub error_message: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn serde_roundtrip() {
        let layout = KeyboardLayout {
            current: "us".to_string(),
            available: vec!["us".to_string(), "cz".to_string(), "de".to_string()],
        };
        let json = serde_json::to_value(&layout).unwrap();
        let decoded: KeyboardLayout = serde_json::from_value(json).unwrap();
        assert_eq!(layout, decoded);
    }

    #[test]
    fn keyboard_layout_config_serde_roundtrip() {
        let config = KeyboardLayoutConfig {
            mode: "editable".to_string(),
            layouts: vec!["us".to_string(), "de".to_string(), "cz".to_string()],
            variant: Some("dvorak".to_string()),
            options: Some("grp:win_space_toggle".to_string()),
            file_path: None,
            error_message: None,
        };
        let json = serde_json::to_value(&config).unwrap();
        let decoded: KeyboardLayoutConfig = serde_json::from_value(json).unwrap();
        assert_eq!(config, decoded);
    }

    #[test]
    fn keyboard_layout_config_external_file_mode() {
        let config = KeyboardLayoutConfig {
            mode: "external-file".to_string(),
            layouts: vec![],
            variant: None,
            options: None,
            file_path: Some("~/.config/keymap.xkb".to_string()),
            error_message: None,
        };
        let json = serde_json::to_value(&config).unwrap();
        let decoded: KeyboardLayoutConfig = serde_json::from_value(json).unwrap();
        assert_eq!(config, decoded);
    }

    #[test]
    fn keyboard_layout_config_error_mode() {
        let config = KeyboardLayoutConfig {
            mode: "error".to_string(),
            layouts: vec![],
            variant: None,
            options: None,
            file_path: None,
            error_message: Some("Config file has syntax errors".to_string()),
        };
        let json = serde_json::to_value(&config).unwrap();
        let decoded: KeyboardLayoutConfig = serde_json::from_value(json).unwrap();
        assert_eq!(config, decoded);
    }
}
