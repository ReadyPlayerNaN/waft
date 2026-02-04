//! Anthropic Admin API client for usage data.

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};

use super::values::{UsageData, UsageResponse, UsageWindow};
use crate::runtime::spawn_on_tokio;

const API_BASE: &str = "https://api.anthropic.com/v1/organizations/usage_report/messages";
const ANTHROPIC_VERSION: &str = "2023-06-01";

/// Fetch usage data from Anthropic Admin API.
///
/// The HTTP request runs on the tokio runtime via [`spawn_on_tokio`] so that
/// this function can be safely awaited from a glib async context without
/// causing busy-polling.
pub async fn fetch_usage(
    api_key: &str,
    window: UsageWindow,
    app_start_time: DateTime<Utc>,
) -> Result<UsageData> {
    let api_key = api_key.to_string();
    let starting_at = window.starting_at(app_start_time);
    let ending_at = Utc::now();
    let bucket_width = window.bucket_width();

    spawn_on_tokio(async move {
        let client = reqwest::Client::new();

        // Format as clean RFC 3339 without fractional seconds: YYYY-MM-DDTHH:MM:SSZ
        let starting_at_str = starting_at.format("%Y-%m-%dT%H:%M:%SZ").to_string();
        let ending_at_str = ending_at.format("%Y-%m-%dT%H:%M:%SZ").to_string();

        let url = format!(
            "{}?starting_at={}&ending_at={}&bucket_width={}",
            API_BASE,
            starting_at_str,
            ending_at_str,
            bucket_width
        );

        log::debug!("[claude-usage] Fetching from URL: {}", url.replace(&api_key, "***"));

        let response = client
            .get(&url)
            .header("x-api-key", &api_key)
            .header("anthropic-version", ANTHROPIC_VERSION)
            .send()
            .await
            .context("Failed to fetch usage data")?;

        // Check for authentication errors
        if response.status() == 401 {
            anyhow::bail!("Authentication failed - check API key");
        }

        // Check for rate limiting
        if response.status() == 429 {
            anyhow::bail!("Rate limited - try again later");
        }

        // Get response text for debugging
        let response_text = response
            .text()
            .await
            .context("Failed to read response body")?;

        log::debug!("[claude-usage] API response: {}", response_text);

        let usage_response: UsageResponse = serde_json::from_str(&response_text)
            .context("Failed to parse usage response")?;

        // Aggregate all time buckets and results
        let mut message_count = 0u64;
        let mut uncached_input_tokens = 0u64;
        let mut cache_read_tokens = 0u64;
        let mut output_tokens = 0u64;
        let mut cache_creation_tokens = 0u64;

        for bucket in usage_response.data {
            for result in bucket.results {
                message_count += 1; // Each result represents one API call/message
                uncached_input_tokens += result.uncached_input_tokens;
                cache_read_tokens += result.cache_read_input_tokens;
                output_tokens += result.output_tokens;

                if let Some(cache_creation) = result.cache_creation {
                    cache_creation_tokens += cache_creation.ephemeral_1h_input_tokens;
                    cache_creation_tokens += cache_creation.ephemeral_5m_input_tokens;
                }
            }
        }

        let input_tokens = uncached_input_tokens + cache_creation_tokens;
        let total_tokens = input_tokens + cache_read_tokens + output_tokens;

        Ok(UsageData {
            window,
            message_count,
            input_tokens,
            output_tokens,
            cache_read_tokens,
            total_tokens,
            timestamp: ending_at,
        })
    })
    .await
}
