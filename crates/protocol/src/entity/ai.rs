use serde::{Deserialize, Serialize};

/// Entity type identifier for AI provider quota data.
pub const ENTITY_TYPE: &str = "provider-usage";
/// Entity type identifier for AI provider settings.
pub const CONFIG_ENTITY_TYPE: &str = "provider-config";

/// Usage data reported by one configured AI provider.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProviderUsage {
    /// Stable provider slug, such as `claude` or `codex`.
    pub provider: String,
    /// Human-readable provider or account name.
    pub display_name: String,
    /// Subscription or account plan name.
    pub plan_name: String,
    /// Whether the provider reports an unlimited plan.
    pub unlimited: bool,
    /// All quota windows reported by the provider.
    pub windows: Vec<ProviderUsageWindow>,
    /// Unix timestamp (ms) when the provider data was fetched.
    pub fetched_at: i64,
    /// Unix timestamp (ms) when the data was originally fetched, if cached.
    pub cached_at: Option<i64>,
}

/// Configuration and credential status for one AI provider.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProviderConfig {
    /// Stable provider slug, such as `claude` or `codex`.
    pub provider: String,
    /// Human-readable provider name.
    pub display_name: String,
    /// Whether quota fetching is enabled for this provider.
    pub enabled: bool,
    /// Whether credentials were found locally.
    pub configured: bool,
}

/// One provider-specific quota window.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProviderUsageWindow {
    /// Provider-defined label, such as `5h`, `weekly`, or `monthly`.
    pub window_type: String,
    pub used: i64,
    pub limit: i64,
    pub remaining: i64,
    /// Unix timestamp (ms) when this window resets.
    pub reset_at: Option<i64>,
    /// Total window length in seconds, when reported by the provider.
    pub period_seconds: Option<i64>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn serde_roundtrip_preserves_windows() {
        let usage = ProviderUsage {
            provider: "claude".to_string(),
            display_name: "Claude".to_string(),
            plan_name: "Max".to_string(),
            unlimited: false,
            windows: vec![ProviderUsageWindow {
                window_type: "5h".to_string(),
                used: 42,
                limit: 100,
                remaining: 58,
                reset_at: Some(1_000_000_000_000),
                period_seconds: Some(18_000),
            }],
            fetched_at: 2_000_000_000_000,
            cached_at: None,
        };
        let json = serde_json::to_value(&usage).expect("expected value");
        let decoded: ProviderUsage = serde_json::from_value(json).expect("expected value");
        assert_eq!(usage, decoded);
    }
}
