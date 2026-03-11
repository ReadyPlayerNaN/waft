use serde::{Deserialize, Serialize};

/// Entity type identifier for AI assistant usage data.
pub const ENTITY_TYPE: &str = "claude-usage";

/// Current Claude Code rate limit utilization.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ClaudeUsage {
    /// 5-hour window utilization
    pub five_hour_utilization: f64,
    /// Unix timestamp (ms) when the 5-hour window resets
    pub five_hour_reset_at: i64,
    /// 7-day window utilization
    pub seven_day_utilization: f64,
    /// Unix timestamp (ms) when the 7-day window resets
    pub seven_day_reset_at: i64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn serde_roundtrip() {
        let usage = ClaudeUsage {
            five_hour_utilization: 42.5,
            five_hour_reset_at: 1_000_000_000_000,
            seven_day_utilization: 85.0,
            seven_day_reset_at: 2_000_000_000_000,
        };
        let json = serde_json::to_value(&usage).unwrap();
        let decoded: ClaudeUsage = serde_json::from_value(json).unwrap();
        assert_eq!(usage, decoded);
    }
}
