//! Data types for Claude usage limits from API headers.

use chrono::{DateTime, Utc};

/// Rate limit information from API response headers.
#[derive(Debug, Clone)]
pub struct RateLimitInfo {
    /// Current usage count
    pub used: u64,
    /// Maximum allowed
    pub limit: u64,
    /// When this limit resets
    pub resets_at: DateTime<Utc>,
}

impl RateLimitInfo {
    /// Calculate utilization percentage (0.0 - 100.0).
    pub fn utilization(&self) -> f64 {
        if self.limit == 0 {
            return 0.0;
        }
        (self.used as f64 / self.limit as f64) * 100.0
    }

    /// Format the reset time as a human-readable string.
    pub fn format_reset_time(&self) -> String {
        let now = Utc::now();
        let duration = self.resets_at.signed_duration_since(now);

        if duration.num_seconds() < 0 {
            return "Now".to_string();
        }

        let hours = duration.num_hours();
        let minutes = duration.num_minutes() % 60;

        if hours > 24 {
            // Show as day/time
            let local_time = self.resets_at.format("%a %H:%M");
            format!("{}", local_time)
        } else if hours > 0 {
            format!("{}h {}min", hours, minutes)
        } else if minutes > 0 {
            format!("{}min", minutes)
        } else {
            "< 1min".to_string()
        }
    }
}

/// Usage data from Admin API rate limit headers.
#[derive(Debug, Clone)]
pub struct UsageData {
    /// Request-based rate limit (usually per-minute)
    pub requests: Option<RateLimitInfo>,
    /// Token-based rate limit (usually per-minute)
    pub tokens: Option<RateLimitInfo>,
}
