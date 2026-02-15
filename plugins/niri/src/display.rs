//! Display output management for Niri.
//!
//! Queries display outputs via `niri msg --json outputs` and provides
//! mode switching and VRR toggle via `niri msg output` commands.

use anyhow::{Context, Result};
use serde::Deserialize;
use std::collections::HashMap;
use waft_protocol::entity::display::{DisplayMode, DisplayOutput};

use crate::commands;
use crate::state::{DisplayOutputState, ModeInfo};

/// Response from `niri msg --json outputs`.
///
/// The response is a map from output name to output info.
pub type NiriOutputsResponse = HashMap<String, NiriOutputInfo>;

/// A single output from Niri's JSON response.
#[derive(Debug, Deserialize)]
pub struct NiriOutputInfo {
    pub name: String,
    pub make: String,
    pub model: String,
    pub modes: Vec<NiriModeInfo>,
    pub current_mode: Option<usize>,
    pub vrr_supported: bool,
    pub vrr_enabled: bool,
}

/// A single display mode from Niri's JSON response.
#[derive(Debug, Deserialize)]
pub struct NiriModeInfo {
    pub width: u32,
    pub height: u32,
    /// Refresh rate in millihertz.
    pub refresh_rate: u32,
    pub is_preferred: bool,
}

/// Query all display outputs from Niri.
pub async fn query_outputs() -> Result<NiriOutputsResponse> {
    commands::niri_msg_json("outputs").await
}

/// Set the display mode for an output.
pub async fn set_mode(output_name: &str, mode_idx: usize) -> Result<()> {
    let idx_str = mode_idx.to_string();
    commands::niri_output(&[output_name, "mode", &idx_str]).await
}

/// Toggle VRR for an output.
pub async fn toggle_vrr(output_name: &str, enable: bool) -> Result<()> {
    let value = if enable { "on" } else { "off" };
    commands::niri_output(&[output_name, "vrr", value]).await
}

/// Convert Niri output response to internal state.
pub fn response_to_states(response: &NiriOutputsResponse) -> HashMap<String, DisplayOutputState> {
    response
        .iter()
        .map(|(name, info)| {
            let state = DisplayOutputState {
                name: info.name.clone(),
                make: info.make.clone(),
                model: info.model.clone(),
                modes: info
                    .modes
                    .iter()
                    .map(|m| ModeInfo {
                        width: m.width,
                        height: m.height,
                        refresh_rate_mhz: m.refresh_rate,
                        preferred: m.is_preferred,
                    })
                    .collect(),
                current_mode_idx: info.current_mode.unwrap_or(0),
                custom_mode: false,
                vrr_supported: info.vrr_supported,
                vrr_enabled: info.vrr_enabled,
            };
            (name.clone(), state)
        })
        .collect()
}

/// Convert display output state to a protocol entity.
pub fn to_entity(state: &DisplayOutputState) -> DisplayOutput {
    let current_mode = state
        .modes
        .get(state.current_mode_idx)
        .map(|m| DisplayMode {
            width: m.width,
            height: m.height,
            refresh_rate: m.refresh_rate_hz(),
            preferred: m.preferred,
        })
        .unwrap_or(DisplayMode {
            width: 0,
            height: 0,
            refresh_rate: 0.0,
            preferred: false,
        });

    let available_modes = state
        .modes
        .iter()
        .map(|m| DisplayMode {
            width: m.width,
            height: m.height,
            refresh_rate: m.refresh_rate_hz(),
            preferred: m.preferred,
        })
        .collect();

    DisplayOutput {
        name: state.name.clone(),
        make: state.make.clone(),
        model: state.model.clone(),
        current_mode,
        available_modes,
        vrr_supported: state.vrr_supported,
        vrr_enabled: state.vrr_enabled,
    }
}

/// Handle actions for a display output entity.
pub async fn handle_action(
    output_name: &str,
    action: &str,
    params: &serde_json::Value,
    state: &DisplayOutputState,
) -> Result<()> {
    match action {
        "set-mode" => {
            let mode_idx = params["mode_index"]
                .as_u64()
                .context("set-mode action requires 'mode_index' parameter")?
                as usize;

            if mode_idx >= state.modes.len() {
                anyhow::bail!(
                    "mode_index {} out of range (output {} has {} modes)",
                    mode_idx,
                    output_name,
                    state.modes.len()
                );
            }

            set_mode(output_name, mode_idx).await
        }
        "toggle-vrr" => {
            let new_state = !state.vrr_enabled;
            toggle_vrr(output_name, new_state).await
        }
        _ => {
            log::debug!("[niri] Unknown display output action: {}", action);
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture_json() -> &'static str {
        include_str!("../tests/fixtures/outputs.json")
    }

    #[test]
    fn test_parse_outputs_response() {
        let response: NiriOutputsResponse = serde_json::from_str(fixture_json()).unwrap();
        assert_eq!(response.len(), 1);

        let dp3 = &response["DP-3"];
        assert_eq!(dp3.name, "DP-3");
        assert_eq!(dp3.make, "Samsung Electric Company");
        assert_eq!(dp3.model, "LS49AG95");
        assert_eq!(dp3.current_mode, Some(0));
        assert!(dp3.vrr_supported);
        assert!(!dp3.vrr_enabled);
        assert!(!dp3.modes.is_empty());
    }

    #[test]
    fn test_parse_mode_refresh_rate_millihertz() {
        let response: NiriOutputsResponse = serde_json::from_str(fixture_json()).unwrap();
        let dp3 = &response["DP-3"];

        // First mode: 5120x1440@239761mHz
        let first_mode = &dp3.modes[0];
        assert_eq!(first_mode.width, 5120);
        assert_eq!(first_mode.height, 1440);
        assert_eq!(first_mode.refresh_rate, 239761);
        assert!(first_mode.is_preferred);
    }

    #[test]
    fn test_response_to_states() {
        let response: NiriOutputsResponse = serde_json::from_str(fixture_json()).unwrap();
        let states = response_to_states(&response);

        assert_eq!(states.len(), 1);
        let dp3 = &states["DP-3"];
        assert_eq!(dp3.name, "DP-3");
        assert_eq!(dp3.current_mode_idx, 0);
        assert!(dp3.vrr_supported);
        assert!(!dp3.vrr_enabled);
    }

    #[test]
    fn test_to_entity() {
        let response: NiriOutputsResponse = serde_json::from_str(fixture_json()).unwrap();
        let states = response_to_states(&response);
        let dp3 = &states["DP-3"];

        let entity = to_entity(dp3);
        assert_eq!(entity.name, "DP-3");
        assert_eq!(entity.make, "Samsung Electric Company");
        assert_eq!(entity.model, "LS49AG95");
        assert_eq!(entity.current_mode.width, 5120);
        assert_eq!(entity.current_mode.height, 1440);
        assert!((entity.current_mode.refresh_rate - 239.761).abs() < 0.001);
        assert!(entity.current_mode.preferred);
        assert!(entity.vrr_supported);
        assert!(!entity.vrr_enabled);
        assert!(!entity.available_modes.is_empty());
    }

    #[test]
    fn test_to_entity_with_fallback_mode() {
        let state = DisplayOutputState {
            name: "HDMI-1".to_string(),
            make: "Unknown".to_string(),
            model: "Unknown".to_string(),
            modes: vec![],
            current_mode_idx: 0,
            custom_mode: false,
            vrr_supported: false,
            vrr_enabled: false,
        };
        let entity = to_entity(&state);
        assert_eq!(entity.current_mode.width, 0);
        assert_eq!(entity.current_mode.height, 0);
    }

    #[test]
    fn test_parse_multi_output() {
        let json = r#"{
            "DP-3": {
                "name": "DP-3",
                "make": "Samsung",
                "model": "S27",
                "serial": "123",
                "physical_size": [600, 340],
                "modes": [{"width": 2560, "height": 1440, "refresh_rate": 60000, "is_preferred": true}],
                "current_mode": 0,
                "is_custom_mode": false,
                "vrr_supported": true,
                "vrr_enabled": false,
                "logical": {"x": 0, "y": 0, "width": 2560, "height": 1440, "scale": 1.0, "transform": "Normal"}
            },
            "eDP-1": {
                "name": "eDP-1",
                "make": "BOE",
                "model": "NV140FHM-N48",
                "serial": "",
                "physical_size": [310, 174],
                "modes": [{"width": 1920, "height": 1080, "refresh_rate": 60002, "is_preferred": true}],
                "current_mode": 0,
                "is_custom_mode": false,
                "vrr_supported": false,
                "vrr_enabled": false,
                "logical": {"x": 2560, "y": 0, "width": 1920, "height": 1080, "scale": 1.0, "transform": "Normal"}
            }
        }"#;

        let response: NiriOutputsResponse = serde_json::from_str(json).unwrap();
        assert_eq!(response.len(), 2);

        let states = response_to_states(&response);
        assert_eq!(states.len(), 2);
        assert!(states.contains_key("DP-3"));
        assert!(states.contains_key("eDP-1"));
    }

    #[test]
    fn test_mode_index_validation_empty() {
        let state = DisplayOutputState {
            name: "DP-3".to_string(),
            make: "".to_string(),
            model: "".to_string(),
            modes: vec![],
            current_mode_idx: 0,
            custom_mode: false,
            vrr_supported: false,
            vrr_enabled: false,
        };

        let params = serde_json::json!({"mode_index": 0});
        let rt = tokio::runtime::Runtime::new().unwrap();
        let result = rt.block_on(handle_action("DP-3", "set-mode", &params, &state));
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("out of range"));
    }
}
