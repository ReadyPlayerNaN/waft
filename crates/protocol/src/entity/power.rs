use serde::{Deserialize, Serialize};

/// Entity type identifier for batteries.
pub const ENTITY_TYPE: &str = "battery";

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn serde_roundtrip() {
        let battery = Battery {
            present: true,
            percentage: 85.0,
            state: BatteryState::Discharging,
            icon_name: "battery-good-symbolic".to_string(),
            time_to_empty: 14400,
            time_to_full: 0,
        };
        let json = serde_json::to_value(&battery).unwrap();
        let decoded: Battery = serde_json::from_value(json).unwrap();
        assert_eq!(battery, decoded);
    }

    #[test]
    fn serde_roundtrip_all_states() {
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
            let json = serde_json::to_value(state).unwrap();
            let decoded: BatteryState = serde_json::from_value(json).unwrap();
            assert_eq!(state, decoded);
        }
    }
}
