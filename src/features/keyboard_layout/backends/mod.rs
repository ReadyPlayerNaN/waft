//! Multi-backend keyboard layout support.
//!
//! This module provides a unified interface for keyboard layout management across
//! different Wayland compositors and fallback to systemd-localed.
//!
//! ## Backend Priority
//!
//! Backends are detected in the following order:
//! 1. **Niri** - Detected via `NIRI_SOCKET` environment variable
//! 2. **Sway** - Detected via `SWAYSOCK` environment variable
//! 3. **Hyprland** - Detected via `HYPRLAND_INSTANCE_SIGNATURE` environment variable
//! 4. **systemd-localed** - Fallback for systems with D-Bus locale1 service

mod hyprland;
mod localed;
mod niri;
mod sway;

use anyhow::Result;
use async_trait::async_trait;
use flume::Sender;
use log::info;
use std::sync::Arc;

use crate::dbus::DbusHandle;

pub use hyprland::HyprlandBackend;
pub use localed::LocaledBackend;
pub use niri::NiriBackend;
pub use sway::SwayBackend;

/// Event emitted when the keyboard layout changes.
#[derive(Debug, Clone)]
pub enum LayoutEvent {
    /// Layout changed - contains the new layout info
    Changed(LayoutInfo),
    /// Error occurred while monitoring
    Error(String),
}

/// Information about the current keyboard layout state.
#[derive(Debug, Clone)]
#[allow(dead_code)] // available and current_index are for future layout picker UI
pub struct LayoutInfo {
    /// Current layout abbreviation (e.g., "US", "CZ")
    pub current: String,
    /// All available layout abbreviations in order
    pub available: Vec<String>,
    /// Index of the current layout in the available list
    pub current_index: usize,
}

/// Trait for keyboard layout backends.
///
/// Each backend implements compositor-specific methods for querying and switching
/// keyboard layouts.
#[async_trait]
#[allow(dead_code)] // switch_prev is for future backward-switch UI action
pub trait KeyboardLayoutBackend: Send + Sync {
    /// Get information about the current keyboard layout state.
    async fn get_layout_info(&self) -> Result<LayoutInfo>;

    /// Switch to the next keyboard layout (wraps around).
    async fn switch_next(&self) -> Result<()>;

    /// Switch to the previous keyboard layout (wraps around).
    async fn switch_prev(&self) -> Result<()>;

    /// Get the backend name for logging purposes.
    fn name(&self) -> &'static str;

    /// Subscribe to layout change events.
    ///
    /// Spawns a background task that monitors for layout changes and sends
    /// events through the provided channel. Returns immediately.
    ///
    /// The background task will run until the sender is dropped or an
    /// unrecoverable error occurs.
    fn subscribe(&self, sender: Sender<LayoutEvent>);
}

/// Extract an abbreviation from a full keyboard layout name.
///
/// Handles various layout name formats:
/// - "English (US)" → "US"
/// - "Czech (QWERTY)" → "CZ" (via country code lookup)
/// - "German" → "DE" (via country code lookup)
/// - "us" → "US" (simple XKB codes)
///
/// # Examples
///
/// ```
/// assert_eq!(extract_abbreviation("English (US)"), "US");
/// assert_eq!(extract_abbreviation("Czech (QWERTY)"), "CZ");
/// assert_eq!(extract_abbreviation("us"), "US");
/// ```
pub fn extract_abbreviation(name: &str) -> String {
    // First, try to extract from parentheses: "English (US)" → "US"
    if let Some(start) = name.find('(') {
        if let Some(end) = name.find(')') {
            if start < end {
                let inside = &name[start + 1..end];
                // Check if it looks like a layout code (2-3 uppercase letters)
                let trimmed = inside.trim();
                if trimmed.len() <= 4
                    && trimmed
                        .chars()
                        .all(|c| c.is_ascii_alphabetic() || c == '-')
                {
                    return trimmed.to_uppercase();
                }
            }
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
        .filter(|c| c.is_ascii_alphabetic())
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
        "german" => Some("DE"),
        "deutsch" => Some("DE"),
        "french" => Some("FR"),
        "francais" | "français" => Some("FR"),
        "spanish" => Some("ES"),
        "español" => Some("ES"),
        "italian" => Some("IT"),
        "italiano" => Some("IT"),
        "portuguese" => Some("PT"),
        "português" => Some("PT"),
        "russian" => Some("RU"),
        "русский" => Some("RU"),
        "czech" => Some("CZ"),
        "čeština" => Some("CZ"),
        "polish" => Some("PL"),
        "polski" => Some("PL"),
        "ukrainian" => Some("UA"),
        "українська" => Some("UA"),
        "japanese" => Some("JP"),
        "日本語" => Some("JP"),
        "chinese" => Some("CN"),
        "中文" => Some("CN"),
        "korean" => Some("KR"),
        "한국어" => Some("KR"),
        "dutch" => Some("NL"),
        "nederlands" => Some("NL"),
        "swedish" => Some("SE"),
        "svenska" => Some("SE"),
        "norwegian" => Some("NO"),
        "norsk" => Some("NO"),
        "danish" => Some("DK"),
        "dansk" => Some("DK"),
        "finnish" => Some("FI"),
        "suomi" => Some("FI"),
        "hungarian" => Some("HU"),
        "magyar" => Some("HU"),
        "greek" => Some("GR"),
        "ελληνικά" => Some("GR"),
        "turkish" => Some("TR"),
        "türkçe" => Some("TR"),
        "arabic" => Some("AR"),
        "العربية" => Some("AR"),
        "hebrew" => Some("IL"),
        "עברית" => Some("IL"),
        "slovak" => Some("SK"),
        "slovenčina" => Some("SK"),
        "slovenian" => Some("SI"),
        "slovenščina" => Some("SI"),
        "croatian" => Some("HR"),
        "hrvatski" => Some("HR"),
        "serbian" => Some("RS"),
        "српски" => Some("RS"),
        "bulgarian" => Some("BG"),
        "български" => Some("BG"),
        "romanian" => Some("RO"),
        "română" => Some("RO"),
        "latvian" => Some("LV"),
        "latviešu" => Some("LV"),
        "lithuanian" => Some("LT"),
        "lietuvių" => Some("LT"),
        "estonian" => Some("EE"),
        "eesti" => Some("EE"),
        _ => None,
    }
}

/// Detect and create the appropriate keyboard layout backend.
///
/// Checks for compositor-specific environment variables in order:
/// 1. NIRI_SOCKET → Niri backend
/// 2. SWAYSOCK → Sway backend
/// 3. HYPRLAND_INSTANCE_SIGNATURE → Hyprland backend
/// 4. D-Bus locale1 available → Localed backend
///
/// Returns `None` if no backend is available.
pub async fn detect_backend(
    dbus: Option<Arc<DbusHandle>>,
) -> Option<Arc<dyn KeyboardLayoutBackend>> {
    // Check for Niri compositor
    if std::env::var("NIRI_SOCKET").is_ok() {
        if let Some(backend) = NiriBackend::new().await {
            info!("[keyboard-layout] Detected Niri compositor, using Niri backend");
            return Some(Arc::new(backend));
        }
    }

    // Check for Sway compositor
    if std::env::var("SWAYSOCK").is_ok() {
        if let Some(backend) = SwayBackend::new().await {
            info!("[keyboard-layout] Detected Sway compositor, using Sway backend");
            return Some(Arc::new(backend));
        }
    }

    // Check for Hyprland compositor
    if std::env::var("HYPRLAND_INSTANCE_SIGNATURE").is_ok() {
        if let Some(backend) = HyprlandBackend::new().await {
            info!("[keyboard-layout] Detected Hyprland compositor, using Hyprland backend");
            return Some(Arc::new(backend));
        }
    }

    // Fallback to systemd-localed via D-Bus
    if let Some(dbus_handle) = dbus {
        if let Some(backend) = LocaledBackend::new(dbus_handle).await {
            info!("[keyboard-layout] Using systemd-localed backend (D-Bus)");
            return Some(Arc::new(backend));
        }
    }

    info!("[keyboard-layout] No keyboard layout backend available");
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_abbreviation_parentheses_us() {
        assert_eq!(extract_abbreviation("English (US)"), "US");
    }

    #[test]
    fn test_extract_abbreviation_parentheses_gb() {
        assert_eq!(extract_abbreviation("English (UK)"), "UK");
    }

    #[test]
    fn test_extract_abbreviation_language_czech() {
        // Czech without parentheses should use language mapping
        assert_eq!(extract_abbreviation("Czech"), "CZ");
    }

    #[test]
    fn test_extract_abbreviation_language_german() {
        assert_eq!(extract_abbreviation("German"), "DE");
    }

    #[test]
    fn test_extract_abbreviation_xkb_code() {
        assert_eq!(extract_abbreviation("us"), "US");
        assert_eq!(extract_abbreviation("de"), "DE");
        assert_eq!(extract_abbreviation("cz"), "CZ");
    }

    #[test]
    fn test_extract_abbreviation_with_variant() {
        // "Czech (QWERTY)" - QWERTY is too long (6 chars), so falls back to language mapping
        assert_eq!(extract_abbreviation("Czech (QWERTY)"), "CZ");
        // "Czech" without variant also uses language mapping
        assert_eq!(extract_abbreviation("Czech"), "CZ");
    }

    #[test]
    fn test_extract_abbreviation_fallback() {
        // Unknown name falls back to first 2 letters
        assert_eq!(extract_abbreviation("Foobar"), "FO");
    }

    #[test]
    fn test_language_to_country_code() {
        assert_eq!(language_to_country_code("english"), Some("EN"));
        assert_eq!(language_to_country_code("german"), Some("DE"));
        assert_eq!(language_to_country_code("czech"), Some("CZ"));
        assert_eq!(language_to_country_code("french"), Some("FR"));
        assert_eq!(language_to_country_code("unknown"), None);
    }
}
