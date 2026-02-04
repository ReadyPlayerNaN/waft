//! Admin API client for rate limit information.

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};

use super::values::{RateLimitInfo, UsageData};
use crate::runtime::spawn_on_tokio;

const ADMIN_API_BASE: &str = "https://api.anthropic.com/v1/organizations";
const ANTHROPIC_VERSION: &str = "2023-06-01";

/// Parse rate limit header value as u64.
fn parse_header_u64(headers: &reqwest::header::HeaderMap, name: &str) -> Option<u64> {
    headers
        .get(name)
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.parse().ok())
}

/// Parse rate limit reset timestamp header.
fn parse_reset_time(headers: &reqwest::header::HeaderMap, name: &str) -> Option<DateTime<Utc>> {
    headers
        .get(name)
        .and_then(|v| v.to_str().ok())
        .and_then(|s| DateTime::parse_from_rfc3339(s).ok())
        .map(|dt| dt.with_timezone(&Utc))
}

/// Extract rate limit info from response headers.
fn extract_rate_limits(headers: &reqwest::header::HeaderMap) -> UsageData {
    // Request limits
    let req_limit = parse_header_u64(headers, "anthropic-ratelimit-requests-limit");
    let req_remaining = parse_header_u64(headers, "anthropic-ratelimit-requests-remaining");
    let req_reset = parse_reset_time(headers, "anthropic-ratelimit-requests-reset");

    let requests = if let (Some(limit), Some(remaining), Some(resets_at)) =
        (req_limit, req_remaining, req_reset)
    {
        Some(RateLimitInfo {
            used: limit.saturating_sub(remaining),
            limit,
            resets_at,
        })
    } else {
        None
    };

    // Token limits
    let tok_limit = parse_header_u64(headers, "anthropic-ratelimit-tokens-limit");
    let tok_remaining = parse_header_u64(headers, "anthropic-ratelimit-tokens-remaining");
    let tok_reset = parse_reset_time(headers, "anthropic-ratelimit-tokens-reset");

    let tokens = if let (Some(limit), Some(remaining), Some(resets_at)) =
        (tok_limit, tok_remaining, tok_reset)
    {
        Some(RateLimitInfo {
            used: limit.saturating_sub(remaining),
            limit,
            resets_at,
        })
    } else {
        None
    };

    UsageData { requests, tokens }
}

/// Fetch rate limit information from Admin API.
///
/// Makes a lightweight API call to /v1/organizations/me and extracts
/// rate limit information from the response headers.
pub async fn fetch_usage(api_key: &str) -> Result<UsageData> {
    let api_key = api_key.to_string();

    spawn_on_tokio(async move {
        let client = reqwest::Client::new();
        let url = format!("{}/me", ADMIN_API_BASE);

        log::debug!("[claude-usage] Fetching rate limits from Admin API");

        let response = client
            .get(&url)
            .header("x-api-key", &api_key)
            .header("anthropic-version", ANTHROPIC_VERSION)
            .send()
            .await
            .context("Failed to fetch from Admin API")?;

        let status = response.status();

        if !status.is_success() {
            anyhow::bail!("Admin API returned status {}", status);
        }

        let headers = response.headers().clone();

        // Log all headers for debugging
        log::debug!("[claude-usage] Response headers:");
        for (name, value) in headers.iter() {
            if name.as_str().starts_with("anthropic-ratelimit") {
                log::debug!("  {}: {:?}", name, value);
            }
        }

        let usage_data = extract_rate_limits(&headers);

        log::debug!(
            "[claude-usage] Rate limits - Requests: {:?}, Tokens: {:?}",
            usage_data.requests.as_ref().map(|r| (r.used, r.limit)),
            usage_data.tokens.as_ref().map(|t| (t.used, t.limit))
        );

        Ok(usage_data)
    })
    .await
}
