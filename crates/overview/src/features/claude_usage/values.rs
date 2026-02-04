//! Data types for Claude usage metrics.

use chrono::{DateTime, Utc};
use serde::Deserialize;

/// Time window for usage tracking.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UsageWindow {
    /// Track usage since application started.
    Session,
    /// Last 60 minutes.
    Hourly,
    /// Last 24 hours.
    Daily,
    /// Last 7 days.
    Weekly,
}

impl UsageWindow {
    /// Parse from string configuration.
    pub fn from_str(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "session" => UsageWindow::Session,
            "hourly" => UsageWindow::Hourly,
            "daily" => UsageWindow::Daily,
            "weekly" => UsageWindow::Weekly,
            _ => {
                log::warn!("Unknown usage window '{}', defaulting to Session", s);
                UsageWindow::Session
            }
        }
    }

    /// Get the bucket width for API requests.
    pub fn bucket_width(&self) -> &'static str {
        match self {
            UsageWindow::Session => "1m",
            UsageWindow::Hourly => "1m",
            UsageWindow::Daily => "1h",
            UsageWindow::Weekly => "1d",
        }
    }

    /// Calculate the starting timestamp for this window.
    pub fn starting_at(&self, app_start_time: DateTime<Utc>) -> DateTime<Utc> {
        match self {
            UsageWindow::Session => app_start_time,
            UsageWindow::Hourly => Utc::now() - chrono::Duration::hours(1),
            UsageWindow::Daily => Utc::now() - chrono::Duration::days(1),
            UsageWindow::Weekly => Utc::now() - chrono::Duration::days(7),
        }
    }

    /// Get display label for this window.
    pub fn label(&self) -> &'static str {
        match self {
            UsageWindow::Session => "Session",
            UsageWindow::Hourly => "Last hour",
            UsageWindow::Daily => "Last 24h",
            UsageWindow::Weekly => "Last 7d",
        }
    }
}

/// Usage data aggregated from the API.
#[derive(Debug, Clone)]
pub struct UsageData {
    pub window: UsageWindow,
    pub message_count: u64,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cache_read_tokens: u64,
    pub total_tokens: u64,
    pub timestamp: DateTime<Utc>,
}

impl UsageData {
    /// Format token count with K/M suffixes.
    pub fn format_tokens(tokens: u64) -> String {
        if tokens >= 1_000_000 {
            format!("{:.1}M", tokens as f64 / 1_000_000.0)
        } else if tokens >= 1_000 {
            format!("{:.1}K", tokens as f64 / 1_000.0)
        } else {
            tokens.to_string()
        }
    }

    /// Format message count with commas.
    pub fn format_messages(count: u64) -> String {
        let s = count.to_string();
        let mut result = String::new();
        let len = s.len();

        for (i, ch) in s.chars().enumerate() {
            if i > 0 && (len - i) % 3 == 0 {
                result.push(',');
            }
            result.push(ch);
        }

        result
    }
}

/// Cache creation tokens from Anthropic Admin API.
#[derive(Debug, Deserialize)]
pub struct CacheCreation {
    #[serde(default)]
    pub ephemeral_1h_input_tokens: u64,
    #[serde(default)]
    pub ephemeral_5m_input_tokens: u64,
}

/// Usage result item from Anthropic Admin API.
#[derive(Debug, Deserialize)]
pub struct UsageResult {
    #[serde(default)]
    pub uncached_input_tokens: u64,
    #[serde(default)]
    pub cache_read_input_tokens: u64,
    #[serde(default)]
    pub output_tokens: u64,
    #[serde(default)]
    pub cache_creation: Option<CacheCreation>,
}

/// Time bucket from Anthropic Admin API.
#[derive(Debug, Deserialize)]
pub struct TimeBucket {
    pub starting_at: String,
    pub ending_at: String,
    pub results: Vec<UsageResult>,
}

/// API response from Anthropic Admin API.
#[derive(Debug, Deserialize)]
pub struct UsageResponse {
    pub data: Vec<TimeBucket>,
    #[serde(default)]
    pub has_more: bool,
    #[serde(default)]
    pub next_page: Option<String>,
}
