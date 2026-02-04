//! Claude Console API client for usage limits.

use anyhow::{Context, Result};

use super::values::{OrganizationInfo, UsageData};
use crate::runtime::spawn_on_tokio;

const ADMIN_API_BASE: &str = "https://api.anthropic.com/v1/organizations";
const CONSOLE_API_BASE: &str = "https://claude.ai/api/organizations";
const ANTHROPIC_VERSION: &str = "2023-06-01";

/// Fetch organization ID from Admin API.
pub async fn fetch_organization_id(api_key: &str) -> Result<String> {
    let api_key = api_key.to_string();

    spawn_on_tokio(async move {
        let client = reqwest::Client::new();
        let url = format!("{}/me", ADMIN_API_BASE);

        log::debug!("[claude-usage] Fetching organization ID");

        let response = client
            .get(&url)
            .header("x-api-key", &api_key)
            .header("anthropic-version", ANTHROPIC_VERSION)
            .send()
            .await
            .context("Failed to fetch organization info")?;

        let status = response.status();
        let response_text = response
            .text()
            .await
            .context("Failed to read response body")?;

        if !status.is_success() {
            anyhow::bail!("Failed to get organization ID: status {}", status);
        }

        let org_info: OrganizationInfo = serde_json::from_str(&response_text)
            .context("Failed to parse organization info")?;

        log::debug!("[claude-usage] Organization: {} ({})", org_info.name, org_info.id);

        Ok(org_info.id)
    })
    .await
}

/// Fetch usage limit data from Claude Console API.
///
/// The HTTP request runs on the tokio runtime via [`spawn_on_tokio`] so that
/// this function can be safely awaited from a glib async context without
/// causing busy-polling.
pub async fn fetch_usage(api_key: &str, org_id: &str) -> Result<UsageData> {
    let api_key = api_key.to_string();
    let org_id = org_id.to_string();

    spawn_on_tokio(async move {
        let client = reqwest::Client::new();
        let url = format!("{}/{}/usage", CONSOLE_API_BASE, org_id);

        log::debug!("[claude-usage] Fetching usage limits");

        let response = client
            .get(&url)
            .header("x-api-key", &api_key)
            .header("anthropic-version", ANTHROPIC_VERSION)
            .send()
            .await
            .context("Failed to fetch usage data")?;

        let status = response.status();
        let response_text = response
            .text()
            .await
            .context("Failed to read response body")?;

        log::debug!("[claude-usage] API response (status {}): {}", status, response_text);

        if !status.is_success() {
            if let Ok(error_json) = serde_json::from_str::<serde_json::Value>(&response_text) {
                if let Some(error_msg) = error_json.get("error")
                    .and_then(|e| e.get("message"))
                    .and_then(|m| m.as_str()) {
                    anyhow::bail!("API error: {}", error_msg);
                }
            }
            anyhow::bail!("API request failed with status {}: {}", status, response_text);
        }

        let usage_data: UsageData = serde_json::from_str(&response_text)
            .context("Failed to parse usage response")?;

        Ok(usage_data)
    })
    .await
}
