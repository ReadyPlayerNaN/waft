use std::collections::HashMap;

use serde::{Deserialize, Serialize};

/// Entity type identifier for displays.
pub const DISPLAY_ENTITY_TYPE: &str = "display";

/// Entity type identifier for display outputs (resolution/mode management).
pub const DISPLAY_OUTPUT_ENTITY_TYPE: &str = "display-output";

/// Entity type identifier for dark mode state.
pub const DARK_MODE_ENTITY_TYPE: &str = "dark-mode";

/// Entity type identifier for night light state.
pub const NIGHT_LIGHT_ENTITY_TYPE: &str = "night-light";

/// Entity type identifier for dark mode automation configuration.
pub const DARK_MODE_AUTOMATION_CONFIG_ENTITY_TYPE: &str = "dark-mode-automation-config";

/// Entity type identifier for night light configuration.
pub const NIGHT_LIGHT_CONFIG_ENTITY_TYPE: &str = "night-light-config";

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

/// A display output with configurable resolution and refresh rate.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DisplayOutput {
    /// Output name (e.g., "DP-3", "HDMI-1", "eDP-1").
    pub name: String,
    /// Manufacturer name.
    pub make: String,
    /// Model name.
    pub model: String,
    /// Currently active display mode.
    pub current_mode: DisplayMode,
    /// All available display modes.
    pub available_modes: Vec<DisplayMode>,
    /// Whether variable refresh rate is supported by the hardware.
    pub vrr_supported: bool,
    /// Whether variable refresh rate is currently enabled.
    pub vrr_enabled: bool,
    /// Whether the output is currently enabled (active).
    pub enabled: bool,
    /// Current scale factor (e.g. 1.0, 1.5, 2.0).
    pub scale: f64,
    /// Current transform/rotation.
    pub transform: DisplayTransform,
    /// Physical size in millimeters [width, height]. None if not reported by EDID.
    pub physical_size: Option<[u32; 2]>,
    /// Connection type derived from output name (e.g. "HDMI", "DisplayPort", "Internal").
    pub connection_type: String,
}

/// Display transform (rotation + optional flip).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DisplayTransform {
    Normal,
    Rotate90,
    Rotate180,
    Rotate270,
    Flipped,
    FlippedRotate90,
    FlippedRotate180,
    FlippedRotate270,
}

impl DisplayTransform {
    /// Decompose into rotation index (0=Normal, 1=90, 2=180, 3=270) and flip state.
    pub fn decompose(self) -> (usize, bool) {
        match self {
            Self::Normal => (0, false),
            Self::Rotate90 => (1, false),
            Self::Rotate180 => (2, false),
            Self::Rotate270 => (3, false),
            Self::Flipped => (0, true),
            Self::FlippedRotate90 => (1, true),
            Self::FlippedRotate180 => (2, true),
            Self::FlippedRotate270 => (3, true),
        }
    }

    /// Compose from rotation index (0=Normal, 1=90, 2=180, 3=270) and flip state.
    pub fn compose(rotation_idx: usize, flipped: bool) -> Self {
        match (rotation_idx % 4, flipped) {
            (0, false) => Self::Normal,
            (1, false) => Self::Rotate90,
            (2, false) => Self::Rotate180,
            (3, false) => Self::Rotate270,
            (0, true) => Self::Flipped,
            (1, true) => Self::FlippedRotate90,
            (2, true) => Self::FlippedRotate180,
            (3, true) => Self::FlippedRotate270,
            _ => unreachable!(),
        }
    }

}

impl DisplayOutput {
    /// Entity type identifier for display outputs.
    pub const ENTITY_TYPE: &'static str = DISPLAY_OUTPUT_ENTITY_TYPE;
}

/// A display mode (resolution + refresh rate).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DisplayMode {
    /// Horizontal resolution in pixels.
    pub width: u32,
    /// Vertical resolution in pixels.
    pub height: u32,
    /// Refresh rate in Hz (e.g., 60.0, 144.0, 239.761).
    pub refresh_rate: f64,
    /// Whether this is the preferred mode for the display.
    pub preferred: bool,
}

/// Dark mode automation configuration entity.
/// Generalized across dark mode switching tools (darkman, Yin-Yang, Blueblack, etc.)
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DarkModeAutomationConfig {
    pub latitude: Option<f64>,
    pub longitude: Option<f64>,
    pub auto_location: Option<bool>,
    pub dbus_api: Option<bool>,
    pub portal_api: Option<bool>,
    pub schema: ConfigSchema,
}

/// Night light configuration entity.
/// Exposes all sunsetr configuration fields organized by category.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NightLightConfig {
    pub target: String,

    // Backend & Mode
    pub backend: String,
    pub transition_mode: String,

    // Colors
    pub night_temp: String,
    pub night_gamma: String,
    pub day_temp: String,
    pub day_gamma: String,
    pub static_temp: String,
    pub static_gamma: String,

    // Timing
    pub sunset: String,
    pub sunrise: String,
    pub transition_duration: String,

    // Location
    pub latitude: String,
    pub longitude: String,

    // Advanced
    pub smoothing: String,
    pub startup_duration: String,
    pub shutdown_duration: String,
    pub adaptive_interval: String,
    pub update_interval: String,

    // Field availability metadata
    pub field_state: HashMap<String, FieldState>,
}

/// Rich schema describing field availability and constraints.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ConfigSchema {
    pub fields: HashMap<String, FieldSchema>,
}

/// Schema metadata for a single configuration field.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FieldSchema {
    pub available: bool,
    pub state: FieldState,
    pub field_type: FieldType,
    pub constraints: Option<Constraints>,
    pub help_text: Option<String>,
}

/// Whether a field is editable, read-only, or disabled.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum FieldState {
    Editable,
    ReadOnly,
    Disabled,
}

/// The data type of a configuration field.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum FieldType {
    Bool,
    Float { decimals: u8 },
    String,
    Enum { options: Vec<String> },
}

/// Numeric constraints for a field.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Constraints {
    pub min: Option<f64>,
    pub max: Option<f64>,
    pub step: Option<f64>,
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

    #[test]
    fn display_output_serde_roundtrip() {
        let output = DisplayOutput {
            name: "DP-3".to_string(),
            make: "Samsung Electric Company".to_string(),
            model: "LS49AG95".to_string(),
            current_mode: DisplayMode {
                width: 5120,
                height: 1440,
                refresh_rate: 239.761,
                preferred: true,
            },
            available_modes: vec![
                DisplayMode {
                    width: 5120,
                    height: 1440,
                    refresh_rate: 239.761,
                    preferred: true,
                },
                DisplayMode {
                    width: 1920,
                    height: 1080,
                    refresh_rate: 60.0,
                    preferred: false,
                },
            ],
            vrr_supported: true,
            vrr_enabled: false,
            enabled: true,
            scale: 1.5,
            transform: DisplayTransform::Rotate90,
            physical_size: Some([1190, 340]),
            connection_type: "DisplayPort".to_string(),
        };
        let json = serde_json::to_value(&output).unwrap();
        let decoded: DisplayOutput = serde_json::from_value(json).unwrap();
        assert_eq!(output, decoded);
    }

    #[test]
    fn display_output_serde_roundtrip_no_physical_size() {
        let output = DisplayOutput {
            name: "HDMI-1".to_string(),
            make: "".to_string(),
            model: "".to_string(),
            current_mode: DisplayMode {
                width: 1920,
                height: 1080,
                refresh_rate: 60.0,
                preferred: true,
            },
            available_modes: vec![],
            vrr_supported: false,
            vrr_enabled: false,
            enabled: false,
            scale: 1.0,
            transform: DisplayTransform::Normal,
            physical_size: None,
            connection_type: "HDMI".to_string(),
        };
        let json = serde_json::to_value(&output).unwrap();
        let decoded: DisplayOutput = serde_json::from_value(json).unwrap();
        assert_eq!(output, decoded);
    }

    #[test]
    fn display_transform_serde_roundtrip_all_variants() {
        let variants = [
            DisplayTransform::Normal,
            DisplayTransform::Rotate90,
            DisplayTransform::Rotate180,
            DisplayTransform::Rotate270,
            DisplayTransform::Flipped,
            DisplayTransform::FlippedRotate90,
            DisplayTransform::FlippedRotate180,
            DisplayTransform::FlippedRotate270,
        ];
        for variant in &variants {
            let json = serde_json::to_value(variant).unwrap();
            let decoded: DisplayTransform = serde_json::from_value(json).unwrap();
            assert_eq!(*variant, decoded);
        }
    }

    #[test]
    fn display_transform_variants_serialize_distinctly() {
        let variants = [
            DisplayTransform::Normal,
            DisplayTransform::Rotate90,
            DisplayTransform::Rotate180,
            DisplayTransform::Rotate270,
            DisplayTransform::Flipped,
            DisplayTransform::FlippedRotate90,
            DisplayTransform::FlippedRotate180,
            DisplayTransform::FlippedRotate270,
        ];
        let serialized: Vec<String> = variants
            .iter()
            .map(|v| serde_json::to_string(v).unwrap())
            .collect();
        let unique: std::collections::HashSet<&String> = serialized.iter().collect();
        assert_eq!(unique.len(), 8, "All 8 transforms must serialize distinctly");
    }

    #[test]
    fn display_transform_decompose_compose_roundtrip() {
        let variants = [
            DisplayTransform::Normal,
            DisplayTransform::Rotate90,
            DisplayTransform::Rotate180,
            DisplayTransform::Rotate270,
            DisplayTransform::Flipped,
            DisplayTransform::FlippedRotate90,
            DisplayTransform::FlippedRotate180,
            DisplayTransform::FlippedRotate270,
        ];
        for variant in &variants {
            let (rotation, flipped) = variant.decompose();
            let recomposed = DisplayTransform::compose(rotation, flipped);
            assert_eq!(*variant, recomposed, "Roundtrip failed for {:?}", variant);
        }
    }

    #[test]
    fn display_mode_serde_roundtrip() {
        let mode = DisplayMode {
            width: 3840,
            height: 2160,
            refresh_rate: 59.94,
            preferred: true,
        };
        let json = serde_json::to_value(&mode).unwrap();
        let decoded: DisplayMode = serde_json::from_value(json).unwrap();
        assert_eq!(mode, decoded);
    }

    #[test]
    fn dark_mode_automation_config_serde_roundtrip() {
        let mut fields = HashMap::new();
        fields.insert(
            "latitude".into(),
            FieldSchema {
                available: true,
                state: FieldState::Editable,
                field_type: FieldType::Float { decimals: 2 },
                constraints: Some(Constraints {
                    min: Some(-90.0),
                    max: Some(90.0),
                    step: Some(0.01),
                }),
                help_text: Some("Test help".into()),
            },
        );

        let config = DarkModeAutomationConfig {
            latitude: Some(50.08),
            longitude: Some(14.42),
            auto_location: Some(true),
            dbus_api: Some(true),
            portal_api: Some(true),
            schema: ConfigSchema { fields },
        };

        let json = serde_json::to_value(&config).unwrap();
        let decoded: DarkModeAutomationConfig = serde_json::from_value(json).unwrap();
        assert_eq!(config, decoded);
    }

    #[test]
    fn field_state_variants_serialize_distinctly() {
        let editable = serde_json::to_string(&FieldState::Editable).unwrap();
        let readonly = serde_json::to_string(&FieldState::ReadOnly).unwrap();
        let disabled = serde_json::to_string(&FieldState::Disabled).unwrap();

        assert_ne!(editable, readonly);
        assert_ne!(readonly, disabled);
    }

    #[test]
    fn field_type_serde_roundtrip() {
        let float_type = FieldType::Float { decimals: 2 };
        let json = serde_json::to_string(&float_type).unwrap();
        let decoded: FieldType = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded, float_type);
    }

    #[test]
    fn night_light_config_serde_roundtrip() {
        let mut field_state = HashMap::new();
        field_state.insert("latitude".into(), FieldState::Editable);
        field_state.insert("static_temp".into(), FieldState::Disabled);

        let config = NightLightConfig {
            target: "default".into(),
            backend: "auto".into(),
            transition_mode: "geo".into(),
            night_temp: "3500".into(),
            night_gamma: "100".into(),
            day_temp: "6500".into(),
            day_gamma: "100".into(),
            static_temp: "4500".into(),
            static_gamma: "100".into(),
            sunset: "20:30:00".into(),
            sunrise: "06:00:00".into(),
            transition_duration: "30".into(),
            latitude: "50.08".into(),
            longitude: "14.42".into(),
            smoothing: "true".into(),
            startup_duration: "1.0".into(),
            shutdown_duration: "1.0".into(),
            adaptive_interval: "100".into(),
            update_interval: "5".into(),
            field_state,
        };

        let json = serde_json::to_value(&config).unwrap();
        let decoded: NightLightConfig = serde_json::from_value(json).unwrap();
        assert_eq!(config, decoded);
    }
}
