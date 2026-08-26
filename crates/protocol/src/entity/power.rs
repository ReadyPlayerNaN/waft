use serde::{Deserialize, Serialize};

/// Entity type identifier for batteries.
pub const ENTITY_TYPE: &str = "battery";

/// Entity type identifier for power profile management.
pub const POWER_PROFILE_ENTITY_TYPE: &str = "power-profile";

/// A battery device (typically laptop battery via UPower).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Battery {
    pub present: bool,
    pub percentage: f64,
    pub state: BatteryState,
    pub icon_name: String,
    pub time_to_empty: i64,
    pub time_to_full: i64,
}

/// Battery charge/discharge state from UPower.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum BatteryState {
    Unknown,
    Charging,
    Discharging,
    Empty,
    FullyCharged,
    PendingCharge,
    PendingDischarge,
}

/// Power profile state from power-profiles-daemon.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PowerProfile {
    pub active_profile: String,
    pub profiles: Vec<String>,
    pub performance_degraded: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn battery_serde_roundtrip() {
        let battery = Battery {
            present: true,
            percentage: 85.0,
            state: BatteryState::Discharging,
            icon_name: "battery-good-symbolic".to_string(),
            time_to_empty: 14400,
            time_to_full: 0,
        };
        let json = serde_json::to_value(&battery).expect("expected value");
        let decoded: Battery = serde_json::from_value(json).expect("expected value");
        assert_eq!(battery, decoded);
    }

    #[test]
    fn battery_state_serde_roundtrip_all_states() {
        let states = [
            BatteryState::Unknown,
            BatteryState::Charging,
            BatteryState::Discharging,
            BatteryState::Empty,
            BatteryState::FullyCharged,
            BatteryState::PendingCharge,
            BatteryState::PendingDischarge,
        ];
        for state in states {
            let json = serde_json::to_value(state).expect("expected value");
            let decoded: BatteryState = serde_json::from_value(json).expect("expected value");
            assert_eq!(state, decoded);
        }
    }

    #[test]
    fn power_profile_serde_roundtrip() {
        let profile = PowerProfile {
            active_profile: "balanced".to_string(),
            profiles: vec![
                "power-saver".to_string(),
                "balanced".to_string(),
                "performance".to_string(),
            ],
            performance_degraded: Some("high-operating-temperature".to_string()),
        };
        let json = serde_json::to_value(&profile).expect("expected value");
        let decoded: PowerProfile = serde_json::from_value(json).expect("expected value");
        assert_eq!(profile, decoded);
    }
}
