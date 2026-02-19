//! Plugin lifecycle entity type.
//!
//! The daemon emits `plugin-status` entities to expose the lifecycle state of
//! discovered plugins. Apps subscribe to this entity type to display plugin
//! status in the settings UI.

use serde::{Deserialize, Serialize};

/// Entity type identifier for plugin status.
pub const ENTITY_TYPE: &str = "plugin-status";

/// Current lifecycle state of a plugin.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum PluginState {
    /// Plugin binary was discovered but has not been spawned.
    Available,
    /// Plugin process is running and connected to the daemon.
    Running,
    /// Plugin was stopped gracefully (via CanStop).
    Stopped,
    /// Plugin crashed and the circuit breaker tripped (too many crashes).
    Failed,
}

impl std::fmt::Display for PluginState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PluginState::Available => write!(f, "Available"),
            PluginState::Running => write!(f, "Running"),
            PluginState::Stopped => write!(f, "Stopped"),
            PluginState::Failed => write!(f, "Failed"),
        }
    }
}

/// Status of a single waft plugin.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PluginStatus {
    /// Human-readable plugin name (e.g. "clock", "bluez", "networkmanager").
    pub name: String,
    /// Current lifecycle state.
    pub state: PluginState,
    /// Entity types this plugin provides (from discovery manifest).
    pub entity_types: Vec<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn serde_roundtrip() {
        let status = PluginStatus {
            name: "clock".to_string(),
            state: PluginState::Running,
            entity_types: vec!["clock".to_string()],
        };
        let json = serde_json::to_value(&status).unwrap();
        let decoded: PluginStatus = serde_json::from_value(json).unwrap();
        assert_eq!(status, decoded);
    }

    #[test]
    fn serde_roundtrip_all_states() {
        let states = [
            PluginState::Available,
            PluginState::Running,
            PluginState::Stopped,
            PluginState::Failed,
        ];
        for state in states {
            let json = serde_json::to_value(&state).unwrap();
            let decoded: PluginState = serde_json::from_value(json).unwrap();
            assert_eq!(state, decoded);
        }
    }

    #[test]
    fn state_serializes_kebab_case() {
        assert_eq!(
            serde_json::to_value(PluginState::Available).unwrap(),
            serde_json::json!("available"),
        );
        assert_eq!(
            serde_json::to_value(PluginState::Running).unwrap(),
            serde_json::json!("running"),
        );
        assert_eq!(
            serde_json::to_value(PluginState::Stopped).unwrap(),
            serde_json::json!("stopped"),
        );
        assert_eq!(
            serde_json::to_value(PluginState::Failed).unwrap(),
            serde_json::json!("failed"),
        );
    }

    #[test]
    fn display_trait() {
        assert_eq!(PluginState::Available.to_string(), "Available");
        assert_eq!(PluginState::Running.to_string(), "Running");
        assert_eq!(PluginState::Stopped.to_string(), "Stopped");
        assert_eq!(PluginState::Failed.to_string(), "Failed");
    }

    #[test]
    fn serde_roundtrip_multiple_entity_types() {
        let status = PluginStatus {
            name: "bluez".to_string(),
            state: PluginState::Available,
            entity_types: vec![
                "bluetooth-adapter".to_string(),
                "bluetooth-device".to_string(),
            ],
        };
        let json = serde_json::to_value(&status).unwrap();
        let decoded: PluginStatus = serde_json::from_value(json).unwrap();
        assert_eq!(status, decoded);
    }
}
