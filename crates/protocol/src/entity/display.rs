use serde::{Deserialize, Serialize};

/// Entity type identifier for displays.
pub const DISPLAY_ENTITY_TYPE: &str = "display";

/// Entity type identifier for dark mode state.
pub const DARK_MODE_ENTITY_TYPE: &str = "dark-mode";

/// Entity type identifier for night light state.
pub const NIGHT_LIGHT_ENTITY_TYPE: &str = "night-light";

/// A display with adjustable brightness.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Display {
    pub name: String,
    pub brightness: f64,
    pub kind: DisplayKind,
}

/// The type of display backend.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DisplayKind {
    Backlight,
    External,
}

/// Dark mode toggle state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct DarkMode {
    pub active: bool,
}

/// Night light (blue light filter) state.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NightLight {
    pub active: bool,
    pub period: Option<String>,
    pub next_transition: Option<String>,
    pub presets: Vec<String>,
    pub active_preset: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn display_serde_roundtrip_backlight() {
        let display = Display {
            name: "intel_backlight".to_string(),
            brightness: 0.8,
            kind: DisplayKind::Backlight,
        };
        let json = serde_json::to_value(&display).unwrap();
        let decoded: Display = serde_json::from_value(json).unwrap();
        assert_eq!(display, decoded);
    }

    #[test]
    fn display_serde_roundtrip_external() {
        let display = Display {
            name: "DELL U2722D".to_string(),
            brightness: 0.5,
            kind: DisplayKind::External,
        };
        let json = serde_json::to_value(&display).unwrap();
        let decoded: Display = serde_json::from_value(json).unwrap();
        assert_eq!(display, decoded);
    }

    #[test]
    fn dark_mode_serde_roundtrip() {
        let mode = DarkMode { active: true };
        let json = serde_json::to_value(mode).unwrap();
        let decoded: DarkMode = serde_json::from_value(json).unwrap();
        assert_eq!(mode, decoded);
    }

    #[test]
    fn night_light_serde_roundtrip() {
        let night_light = NightLight {
            active: true,
            period: Some("night".to_string()),
            next_transition: Some("06:30".to_string()),
            presets: vec!["warm".to_string(), "cool".to_string()],
            active_preset: Some("warm".to_string()),
        };
        let json = serde_json::to_value(&night_light).unwrap();
        let decoded: NightLight = serde_json::from_value(json).unwrap();
        assert_eq!(night_light, decoded);
    }

    #[test]
    fn night_light_serde_roundtrip_inactive() {
        let night_light = NightLight {
            active: false,
            period: None,
            next_transition: None,
            presets: vec![],
            active_preset: None,
        };
        let json = serde_json::to_value(&night_light).unwrap();
        let decoded: NightLight = serde_json::from_value(json).unwrap();
        assert_eq!(night_light, decoded);
    }
}
