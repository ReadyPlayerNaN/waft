//! Internal state types for the Niri plugin.

use std::collections::HashMap;

use crate::config::KeyboardConfig;

/// Combined plugin state for keyboard layouts and display outputs.
#[derive(Debug, Default)]
pub struct NiriState {
    pub keyboard: KeyboardLayoutState,
    pub keyboard_config: KeyboardConfig,
    pub outputs: HashMap<String, DisplayOutputState>,
}

/// Keyboard layout state tracked from Niri events.
#[derive(Debug, Default)]
pub struct KeyboardLayoutState {
    /// Full layout names from Niri (e.g., "English (US)", "Czech (QWERTY)").
    pub names: Vec<String>,
    /// Index of the currently active layout.
    pub current_idx: usize,
}

/// State for a single display output.
#[derive(Debug, Clone)]
pub struct DisplayOutputState {
    /// Output connector name (e.g., "DP-3").
    pub name: String,
    /// Manufacturer name.
    pub make: String,
    /// Model name.
    pub model: String,
    /// Available display modes.
    pub modes: Vec<ModeInfo>,
    /// Index of the current mode in the modes list.
    pub current_mode_idx: usize,
    /// Whether the current mode is a custom (user-specified) mode.
    pub custom_mode: bool,
    /// Whether VRR is supported by the hardware.
    pub vrr_supported: bool,
    /// Whether VRR is currently enabled.
    pub vrr_enabled: bool,
    /// Whether the output is currently enabled (logical is Some in niri).
    pub enabled: bool,
    /// Current scale factor.
    pub scale: f64,
    /// Current transform string from niri (e.g. "Normal", "90", "Flipped90").
    pub transform: String,
    /// Physical size in millimeters [width, height]. None if not reported.
    pub physical_size: Option<[u32; 2]>,
}

/// A single display mode from Niri.
#[derive(Debug, Clone)]
pub struct ModeInfo {
    pub width: u32,
    pub height: u32,
    /// Refresh rate in millihertz (Niri's native format).
    pub refresh_rate_mhz: u32,
    pub preferred: bool,
}

impl ModeInfo {
    /// Convert millihertz to Hz (e.g., 239761 -> 239.761).
    pub fn refresh_rate_hz(&self) -> f64 {
        self.refresh_rate_mhz as f64 / 1000.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn refresh_rate_conversion() {
        let mode = ModeInfo {
            width: 1920,
            height: 1080,
            refresh_rate_mhz: 60000,
            preferred: true,
        };
        assert!((mode.refresh_rate_hz() - 60.0).abs() < 0.001);
    }

    #[test]
    fn refresh_rate_conversion_fractional() {
        let mode = ModeInfo {
            width: 5120,
            height: 1440,
            refresh_rate_mhz: 239761,
            preferred: true,
        };
        assert!((mode.refresh_rate_hz() - 239.761).abs() < 0.001);
    }

    #[test]
    fn refresh_rate_conversion_ntsc() {
        let mode = ModeInfo {
            width: 1920,
            height: 1080,
            refresh_rate_mhz: 59940,
            preferred: false,
        };
        assert!((mode.refresh_rate_hz() - 59.94).abs() < 0.001);
    }
}
