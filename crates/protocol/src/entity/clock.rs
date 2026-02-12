use serde::{Deserialize, Serialize};

/// Entity type identifier for clocks.
pub const ENTITY_TYPE: &str = "clock";

/// Current time and date, formatted for display.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Clock {
    pub time: String,
    pub date: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn serde_roundtrip() {
        let clock = Clock {
            time: "14:30".to_string(),
            date: "Thursday, 12 Feb 2026".to_string(),
        };
        let json = serde_json::to_value(&clock).unwrap();
        let decoded: Clock = serde_json::from_value(json).unwrap();
        assert_eq!(clock, decoded);
    }
}
