//! Claude Code usage daemon.
//!
//! Reads OAuth credentials from ~/.claude/.credentials.json on each poll cycle
//! and fetches rate limit data from https://api.anthropic.com/api/oauth/usage.
//! Never modifies the credentials file (avoids rotating refresh tokens).
//!
//! No configuration required — zero-config.

use std::sync::{Arc, Mutex as StdMutex};
use std::time::Duration;

use anyhow::Result;
use waft_plugin::*;

use waft_plugin_claude::{api, credentials};

const POLL_INTERVAL_SECS: u64 = 300; // 5 minutes

struct ClaudePlugin {
    state: Arc<StdMutex<Option<api::UsageData>>>,
}

impl ClaudePlugin {
    fn new() -> Self {
        Self {
            state: Arc::new(StdMutex::new(None)),
        }
    }
}

#[async_trait::async_trait]
impl Plugin for ClaudePlugin {
    fn get_entities(&self) -> Vec<Entity> {
        let guard = match self.state.lock() {
            Ok(g) => g,
            Err(e) => e.into_inner(),
        };
        let Some(ref data) = *guard else {
            return vec![];
        };

        let usage = entity::ai::ClaudeUsage {
            five_hour_utilization: data.five_hour_utilization,
            five_hour_reset_at: data.five_hour_reset_at,
            seven_day_utilization: data.seven_day_utilization,
            seven_day_reset_at: data.seven_day_reset_at,
        };

        vec![Entity::new(
            Urn::new("claude", entity::ai::ENTITY_TYPE, "me"),
            entity::ai::ENTITY_TYPE,
            &usage,
        )]
    }

    async fn handle_action(
        &self,
        _urn: Urn,
        action: String,
        _params: serde_json::Value,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        Err(format!("Unknown action: {action}").into())
    }
}

fn main() -> Result<()> {
    PluginRunner::new("claude", &[entity::ai::ENTITY_TYPE])
        .meta("Claude Code", "Claude Code rate limit utilization")
        .run(|notifier| async move {
            let plugin = ClaudePlugin::new();
            let state = plugin.state.clone();

            spawn_monitored_anyhow("claude-poll", async move {
                loop {
                    match credentials::load_access_token() {
                        Err(e) => {
                            log::warn!("[claude] Skipping poll: {e}");
                        }
                        Ok(token) => match api::fetch_usage(&token).await {
                            Ok(data) => {
                                log::debug!(
                                    "[claude] Usage: 5h={:.1}% 7d={:.1}%",
                                    data.five_hour_utilization,
                                    data.seven_day_utilization,
                                );
                                *match state.lock() {
                                    Ok(g) => g,
                                    Err(e) => e.into_inner(),
                                } = Some(data);
                                notifier.notify();
                            }
                            Err(e) => {
                                log::error!("[claude] Failed to fetch usage: {e}");
                            }
                        },
                    }

                    tokio::time::sleep(Duration::from_secs(POLL_INTERVAL_SECS)).await;
                }
            });

            Ok(plugin)
        })
}
