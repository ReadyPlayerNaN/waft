//! Anthropic OAuth usage API client.

use anyhow::{Context, Result};
use serde::Deserialize;

const USAGE_URL: &str = "https://api.anthropic.com/api/oauth/usage";
const BETA_HEADER: &str = "oauth-2025-04-20";

#[derive(Debug, Deserialize)]
struct UsageWindow {
    /// Utilization as a percentage, 0.0–100.0
    utilization: f64,
    /// ISO 8601 timestamp when the window resets (e.g. "2026-03-11T19:00:00.224373+00:00")
    resets_at: String,
}

#[derive(Debug, Deserialize)]
struct UsageResponse {
    five_hour: UsageWindow,
    seven_day: UsageWindow,
}

/// Parsed usage data with utilization (0.0–100.0) and Unix-ms reset timestamps.
#[derive(Debug, Clone)]
pub struct UsageData {
    pub five_hour_utilization: f64,
    pub five_hour_reset_at: i64,
    pub seven_day_utilization: f64,
    pub seven_day_reset_at: i64,
}

/// Fetch Claude Code rate limit utilization using the provided OAuth access token.
pub async fn fetch_usage(access_token: &str) -> Result<UsageData> {
    let client = reqwest::Client::new();
    let response = client
        .get(USAGE_URL)
        .bearer_auth(access_token)
        .header("anthropic-beta", BETA_HEADER)
        .send()
        .await
        .context("Failed to send usage request")?;

    let status = response.status();
    if !status.is_success() {
        let body = response.text().await.unwrap_or_default();
        anyhow::bail!("Usage API returned {status}: {body}");
    }

    let body = response
        .text()
        .await
        .context("Failed to read usage response body")?;

    let data: UsageResponse = serde_json::from_str(&body).with_context(|| {
        format!("Failed to parse usage response: {body}")
    })?;

    Ok(UsageData {
        five_hour_utilization: data.five_hour.utilization,
        five_hour_reset_at: parse_reset_at(&data.five_hour.resets_at)?,
        seven_day_utilization: data.seven_day.utilization,
        seven_day_reset_at: parse_reset_at(&data.seven_day.resets_at)?,
    })
}

/// Parse an ISO 8601 UTC timestamp string to Unix milliseconds.
///
/// Supports formats returned by the API:
/// - "2026-03-11T19:00:00.224373+00:00" (offset with sub-second precision)
/// - "2026-03-11T14:00:00Z" (trailing Z)
fn parse_reset_at(s: &str) -> Result<i64> {
    let s = s.trim_end_matches('Z');
    let (date_part, time_part) = s.split_once('T').context("Invalid timestamp format")?;

    let mut date_parts = date_part.split('-');
    let year: i64 = date_parts.next().context("Missing year")?.parse()?;
    let month: i64 = date_parts.next().context("Missing month")?.parse()?;
    let day: i64 = date_parts.next().context("Missing day")?.parse()?;

    let time_part = time_part.split('+').next().unwrap_or(time_part);
    let mut time_parts = time_part.split(':');
    let hour: i64 = time_parts.next().context("Missing hour")?.parse()?;
    let minute: i64 = time_parts.next().context("Missing minute")?.parse()?;
    let second: i64 = time_parts
        .next()
        .unwrap_or("0")
        .split('.')
        .next()
        .unwrap_or("0")
        .parse()?;

    let days = days_since_epoch(year, month, day);
    let unix_secs = days * 86400 + hour * 3600 + minute * 60 + second;
    Ok(unix_secs * 1000)
}

/// Compute days since 1970-01-01 for a Gregorian date.
///
/// Algorithm: <https://howardhinnant.github.io/date_algorithms.html>
fn days_since_epoch(year: i64, month: i64, day: i64) -> i64 {
    let y = if month <= 2 { year - 1 } else { year };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let doy = (153 * (if month > 2 { month - 3 } else { month + 9 }) + 2) / 5 + day - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146097 + doe - 719468
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_reset_at_utc() {
        let ts = parse_reset_at("2025-03-11T14:00:00Z").unwrap();
        assert_eq!(ts, 1_741_701_600_000);
    }

    #[test]
    fn parse_reset_at_midnight() {
        let ts = parse_reset_at("1970-01-01T00:00:00Z").unwrap();
        assert_eq!(ts, 0);
    }

    #[test]
    fn parse_reset_at_with_offset_and_subseconds() {
        // Actual format returned by the API: sub-second precision + numeric offset
        let ts = parse_reset_at("2025-03-11T14:00:00.224373+00:00").unwrap();
        assert_eq!(ts, 1_741_701_600_000);
    }
}
