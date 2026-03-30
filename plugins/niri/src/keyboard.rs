//! Keyboard layout management for Niri.
//!
//! Migrated from `plugins/keyboard-layout/src/backends/niri.rs`.
//! Queries and switches keyboard layouts via `niri msg` commands.

use anyhow::Result;
use serde::Deserialize;
use waft_protocol::entity::keyboard::{KeyboardLayout, KeyboardLayoutConfig as ProtoConfig};

use crate::commands;
use crate::config::{KeyboardConfig, KeyboardConfigMode};
use crate::state::KeyboardLayoutState;

/// Response from `niri msg --json keyboard-layouts`.
#[derive(Debug, Deserialize)]
pub struct NiriLayoutsResponse {
    /// Layout names (e.g., ["English (US)", "Czech (QWERTY)"])
    pub names: Vec<String>,
    /// Index of the currently active layout
    pub current_idx: usize,
}

/// Query keyboard layouts from Niri.
pub async fn query_layouts() -> Result<NiriLayoutsResponse> {
    commands::niri_msg_json("keyboard-layouts").await
}

/// Switch to the next keyboard layout.
pub async fn switch_next() -> Result<()> {
    commands::niri_action(&["switch-layout", "next"]).await
}

/// Switch to a specific keyboard layout by index.
pub async fn switch_to(index: usize) -> Result<()> {
    commands::niri_action(&["switch-layout", &index.to_string()]).await
}

/// Convert keyboard layout state to a protocol entity.
pub fn to_entity(state: &KeyboardLayoutState) -> KeyboardLayout {
    let available: Vec<String> = state
        .names
        .iter()
        .map(|n| extract_abbreviation(n))
        .collect();

    let current_index = state.current_idx.min(available.len().saturating_sub(1));
    let current = available
        .get(current_index)
        .cloned()
        .unwrap_or_else(|| "??".to_string());

    KeyboardLayout { current, available }
}

/// Update state from a layouts response.
pub fn update_state_from_response(state: &mut KeyboardLayoutState, response: &NiriLayoutsResponse) {
    state.names = response.names.clone();
    state.current_idx = response.current_idx;
}

/// Convert keyboard config state to a protocol entity.
pub fn to_config_entity(config: &KeyboardConfig) -> ProtoConfig {
    let mode_str = match config.mode {
        KeyboardConfigMode::LayoutList => "editable",
        KeyboardConfigMode::ExternalFile => "external-file",
        KeyboardConfigMode::SystemDefault => "system-default",
        KeyboardConfigMode::Malformed => "error",
    };

    ProtoConfig {
        mode: mode_str.to_string(),
        layouts: config.layouts.clone(),
        layout_names: config.layout_names.clone(),
        variant: config.variant.clone(),
        options: config.options.clone(),
        file_path: config.file_path.clone(),
        error_message: config.error_message.clone(),
    }
}

/// Extract an abbreviation from a full keyboard layout name.
///
/// Handles various layout name formats:
/// - "English (US)" -> "US"
/// - "Czech (QWERTY)" -> "CZ" (via country code lookup)
/// - "German" -> "DE" (via country code lookup)
/// - "us" -> "US" (simple XKB codes)
pub fn extract_abbreviation(name: &str) -> String {
    // First, try to extract from parentheses: "English (US)" -> "US"
    if let Some(start) = name.find('(')
        && let Some(end) = name.find(')')
        && start < end
    {
        let inside = &name[start + 1..end];
        let trimmed = inside.trim();
        if trimmed.len() <= 4 && trimmed.chars().all(|c| c.is_ascii_alphabetic() || c == '-') {
            return trimmed.to_uppercase();
        }
    }

    // Try to match language name to country code
    let name_lower = name.to_lowercase();
    let first_word = name_lower.split_whitespace().next().unwrap_or(&name_lower);

    if let Some(code) = language_to_country_code(first_word) {
        return code.to_string();
    }

    // Check if the name itself is a short XKB code
    if name.len() <= 3 && name.chars().all(|c| c.is_ascii_alphabetic()) {
        return name.to_uppercase();
    }

    // Fallback: use first 2 chars uppercase
    name.chars()
        .filter(char::is_ascii_alphabetic)
        .take(2)
        .collect::<String>()
        .to_uppercase()
}

/// Map language names to country codes.
fn language_to_country_code(language: &str) -> Option<&'static str> {
    match language {
        "english" => Some("EN"),
        "american" => Some("US"),
        "british" => Some("GB"),
        "german" | "deutsch" => Some("DE"),
        "french" | "francais" | "fran\u{e7}ais" => Some("FR"),
        "spanish" | "espa\u{f1}ol" => Some("ES"),
        "italian" | "italiano" => Some("IT"),
        "portuguese" | "portugu\u{ea}s" => Some("PT"),
        "russian" | "\u{440}\u{443}\u{441}\u{441}\u{43a}\u{438}\u{439}" => Some("RU"),
        "czech" | "\u{10d}e\u{161}tina" => Some("CZ"),
        "polish" | "polski" => Some("PL"),
        "ukrainian" | "\u{443}\u{43a}\u{440}\u{430}\u{457}\u{43d}\u{441}\u{44c}\u{43a}\u{430}" => {
            Some("UA")
        }
        "japanese" | "\u{65e5}\u{672c}\u{8a9e}" => Some("JP"),
        "chinese" | "\u{4e2d}\u{6587}" => Some("CN"),
        "korean" | "\u{d55c}\u{ad6d}\u{c5b4}" => Some("KR"),
        "dutch" | "nederlands" => Some("NL"),
        "swedish" | "svenska" => Some("SE"),
        "norwegian" | "norsk" => Some("NO"),
        "danish" | "dansk" => Some("DK"),
        "finnish" | "suomi" => Some("FI"),
        "hungarian" | "magyar" => Some("HU"),
        "greek" | "\u{3b5}\u{3bb}\u{3bb}\u{3b7}\u{3bd}\u{3b9}\u{3ba}\u{3ac}" => Some("GR"),
        "turkish" | "t\u{fc}rk\u{e7}e" => Some("TR"),
        "arabic" | "\u{627}\u{644}\u{639}\u{631}\u{628}\u{64a}\u{629}" => Some("AR"),
        "hebrew" | "\u{5e2}\u{5d1}\u{5e8}\u{5d9}\u{5ea}" => Some("IL"),
        "slovak" | "sloven\u{10d}ina" => Some("SK"),
        "slovenian" | "sloven\u{161}\u{10d}ina" => Some("SI"),
        "croatian" | "hrvatski" => Some("HR"),
        "serbian" | "\u{441}\u{440}\u{43f}\u{441}\u{43a}\u{438}" => Some("RS"),
        "bulgarian" | "\u{431}\u{44a}\u{43b}\u{433}\u{430}\u{440}\u{441}\u{43a}\u{438}" => {
            Some("BG")
        }
        "romanian" | "rom\u{e2}n\u{103}" => Some("RO"),
        "latvian" | "latvie\u{161}u" => Some("LV"),
        "lithuanian" | "lietuvi\u{173}" => Some("LT"),
        "estonian" | "eesti" => Some("EE"),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn config_to_entity_editable_mode() {
        let config = KeyboardConfig {
            mode: KeyboardConfigMode::LayoutList,
            layouts: vec!["us".into(), "de".into()],
            layout_names: vec!["English (US)".into(), "German".into()],
            variant: Some("dvorak".into()),
            options: Some("grp:win_space_toggle".into()),
            file_path: None,
            error_message: None,
        };

        let entity = to_config_entity(&config);
        assert_eq!(entity.mode, "editable");
        assert_eq!(entity.layouts, vec!["us", "de"]);
        assert_eq!(entity.layout_names, vec!["English (US)", "German"]);
        assert_eq!(entity.variant, Some("dvorak".to_string()));
        assert_eq!(entity.options, Some("grp:win_space_toggle".to_string()));
    }

    #[test]
    fn config_to_entity_external_file_mode() {
        let config = KeyboardConfig {
            mode: KeyboardConfigMode::ExternalFile,
            layouts: vec![],
            layout_names: vec![],
            variant: None,
            options: None,
            file_path: Some("~/.config/keymap.xkb".into()),
            error_message: None,
        };

        let entity = to_config_entity(&config);
        assert_eq!(entity.mode, "external-file");
        assert_eq!(entity.file_path, Some("~/.config/keymap.xkb".to_string()));
    }

    #[test]
    fn config_to_entity_error_mode() {
        let config = KeyboardConfig {
            mode: KeyboardConfigMode::Malformed,
            layouts: vec![],
            layout_names: vec![],
            variant: None,
            options: None,
            file_path: None,
            error_message: Some("Parse error".into()),
        };

        let entity = to_config_entity(&config);
        assert_eq!(entity.mode, "error");
        assert_eq!(entity.error_message, Some("Parse error".to_string()));
    }

    #[test]
    fn config_to_entity_system_default_mode() {
        let config = KeyboardConfig::default();
        let entity = to_config_entity(&config);
        assert_eq!(entity.mode, "system-default");
        assert!(entity.layouts.is_empty());
    }

    #[test]
    fn test_parse_niri_response() {
        let json = r#"{"names":["English (US)","Czech (QWERTY)"],"current_idx":0}"#;
        let response: NiriLayoutsResponse = serde_json::from_str(json).unwrap();

        assert_eq!(response.names.len(), 2);
        assert_eq!(response.names[0], "English (US)");
        assert_eq!(response.names[1], "Czech (QWERTY)");
        assert_eq!(response.current_idx, 0);
    }

    #[test]
    fn test_parse_niri_response_single_layout() {
        let json = r#"{"names":["English (US)"],"current_idx":0}"#;
        let response: NiriLayoutsResponse = serde_json::from_str(json).unwrap();

        assert_eq!(response.names.len(), 1);
        assert_eq!(response.current_idx, 0);
    }

    #[test]
    fn test_extract_abbreviation_parentheses() {
        assert_eq!(extract_abbreviation("English (US)"), "US");
        assert_eq!(extract_abbreviation("English (UK)"), "UK");
    }

    #[test]
    fn test_extract_abbreviation_language() {
        assert_eq!(extract_abbreviation("Czech"), "CZ");
        assert_eq!(extract_abbreviation("German"), "DE");
    }

    #[test]
    fn test_extract_abbreviation_xkb() {
        assert_eq!(extract_abbreviation("us"), "US");
        assert_eq!(extract_abbreviation("de"), "DE");
    }

    #[test]
    fn test_extract_abbreviation_with_variant() {
        // "Czech (QWERTY)" - QWERTY is too long, falls back to language mapping
        assert_eq!(extract_abbreviation("Czech (QWERTY)"), "CZ");
    }

    #[test]
    fn test_to_entity() {
        let state = KeyboardLayoutState {
            names: vec!["English (US)".to_string(), "Czech (QWERTY)".to_string()],
            current_idx: 0,
        };
        let entity = to_entity(&state);
        assert_eq!(entity.current, "US");
        assert_eq!(entity.available, vec!["US", "CZ"]);
    }

    #[test]
    fn test_to_entity_second_active() {
        let state = KeyboardLayoutState {
            names: vec!["English (US)".to_string(), "German".to_string()],
            current_idx: 1,
        };
        let entity = to_entity(&state);
        assert_eq!(entity.current, "DE");
    }

    #[test]
    fn test_to_entity_empty_names() {
        let state = KeyboardLayoutState {
            names: vec![],
            current_idx: 0,
        };
        let entity = to_entity(&state);
        assert_eq!(entity.current, "??");
        assert!(entity.available.is_empty());
    }

    #[test]
    fn test_update_state_from_response() {
        let mut state = KeyboardLayoutState::default();
        let response = NiriLayoutsResponse {
            names: vec!["English (US)".to_string(), "Czech".to_string()],
            current_idx: 1,
        };
        update_state_from_response(&mut state, &response);
        assert_eq!(state.names.len(), 2);
        assert_eq!(state.current_idx, 1);
    }
}
