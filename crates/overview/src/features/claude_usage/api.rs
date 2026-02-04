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

        let url = format!(
            "{}?starting_at={}&ending_at={}&bucket_width={}",
            API_BASE,
            starting_at.to_rfc3339(),
            ending_at.to_rfc3339(),
            bucket_width
        );

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

        let usage_response: UsageResponse = response
            .json()
            .await
            .context("Failed to parse usage response")?;

        // Aggregate all buckets
        let mut message_count = 0u64;
        let mut input_tokens = 0u64;
        let mut output_tokens = 0u64;
        let mut cache_read_tokens = 0u64;

        for bucket in usage_response.buckets {
            message_count += bucket.count;
            input_tokens += bucket.input_tokens;
            output_tokens += bucket.output_tokens;
            cache_read_tokens += bucket.cache_read_tokens;
        }

        let total_tokens = input_tokens + output_tokens + cache_read_tokens;

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
