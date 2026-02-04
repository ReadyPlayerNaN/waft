//! Data types for Claude usage limits.

use chrono::{DateTime, Utc};
use serde::Deserialize;

/// Usage limit information for a specific window.
#[derive(Debug, Clone, Deserialize)]
pub struct LimitInfo {
    /// Utilization percentage (0.0 - 100.0)
    pub utilization: f64,
    /// When this limit resets
    pub resets_at: DateTime<Utc>,
}

impl LimitInfo {
    /// Format the reset time as a human-readable string.
    pub fn format_reset_time(&self) -> String {
        let now = Utc::now();
        let duration = self.resets_at.signed_duration_since(now);

        if duration.num_seconds() < 0 {
            return "Reset past".to_string();
        }

        let hours = duration.num_hours();
        let minutes = duration.num_minutes() % 60;

        if hours > 24 {
            // Show as day/time
            let local_time = self.resets_at.format("%a %H:%M");
            format!("{}", local_time)
        } else if hours > 0 {
            format!("{}h {}min", hours, minutes)
        } else {
            format!("{}min", minutes)
        }
    }
}

/// Usage data from Claude Console API.
#[derive(Debug, Clone, Deserialize)]
pub struct UsageData {
    /// 5-hour session limit
    pub five_hour: Option<LimitInfo>,
    /// 7-day weekly limit (all models)
    pub seven_day: Option<LimitInfo>,
    /// 7-day limit for Sonnet only
    pub seven_day_sonnet: Option<LimitInfo>,
    /// 7-day limit for Opus only
    pub seven_day_opus: Option<LimitInfo>,
    /// 7-day OAuth apps limit
    pub seven_day_oauth_apps: Option<LimitInfo>,
    /// 7-day cowork limit
    pub seven_day_cowork: Option<LimitInfo>,
}

/// Organization info from Admin API.
#[derive(Debug, Deserialize)]
pub struct OrganizationInfo {
    pub id: String,
    #[serde(rename = "type")]
    pub org_type: String,
    pub name: String,
}
