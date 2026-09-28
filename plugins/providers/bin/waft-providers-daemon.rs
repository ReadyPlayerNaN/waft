//! AI provider quota daemon.
//!
//! The `quotas` crate owns provider-specific authentication and HTTP parsing.
//! This daemon owns the Waft lifecycle, polling, and protocol adaptation.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use anyhow::Result;
use futures::future::join_all;
use quotas::auth::cursor::CursorAuthResolver;
use quotas::auth::env::EnvResolver;
use quotas::auth::file::{CookieFileResolver, FileResolver};
use quotas::auth::oauth::OAuthFileResolver;
use quotas::auth::opencode::{KimiCliResolver, OpencodeAuthResolver, OpencodeSlot};
use quotas::auth::pi::PiAuthResolver;
use quotas::auth::{AuthResolver, MultiResolver};
use quotas::providers::{Provider, ProviderKind, ProviderResult, ProviderStatus};
use waft_plugin::*;
use waft_protocol::entity::ai::{
    CONFIG_ENTITY_TYPE, ENTITY_TYPE, ProviderConfig, ProviderUsage, ProviderUsageWindow,
};

const POLL_INTERVAL_SECS: u64 = 300;

#[derive(Debug, Clone, Default, serde::Deserialize, serde::Serialize)]
struct ProviderSettings {
    #[serde(default)]
    enabled: BTreeMap<String, bool>,
}

impl ProviderSettings {
    fn path() -> PathBuf {
        dirs::config_dir()
            .unwrap_or_else(|| PathBuf::from("~/.config"))
            .join("waft/providers.toml")
    }

    fn load() -> Self {
        let path = Self::path();
        match std::fs::read_to_string(&path) {
            Ok(content) => toml::from_str(&content).unwrap_or_else(|error| {
                log::warn!("[providers] failed to parse {}: {error}", path.display());
                Self::default()
            }),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Self::default(),
            Err(error) => {
                log::warn!("[providers] failed to read {}: {error}", path.display());
                Self::default()
            }
        }
    }

    fn is_enabled(&self, kind: ProviderKind) -> bool {
        self.enabled.get(kind.slug()).copied().unwrap_or(true)
    }

    fn set_enabled(&mut self, kind: ProviderKind, enabled: bool) {
        self.enabled.insert(kind.slug().to_string(), enabled);
    }

    fn save(&self) -> anyhow::Result<()> {
        let path = Self::path();
        let parent = path
            .parent()
            .ok_or_else(|| anyhow::anyhow!("provider settings path has no parent"))?;
        std::fs::create_dir_all(parent)?;
        let content = toml::to_string_pretty(self)?;
        let temporary = path.with_extension("toml.tmp");
        std::fs::write(&temporary, content)?;
        std::fs::rename(&temporary, &path)?;
        Ok(())
    }
}

struct ProviderSpec {
    kind: ProviderKind,
    provider: Box<dyn Provider>,
}

struct ProvidersPlugin {
    state: Arc<Mutex<BTreeMap<String, ProviderResult>>>,
    settings: Arc<Mutex<ProviderSettings>>,
    providers: Arc<Vec<ProviderSpec>>,
    notifier: EntityNotifier,
}

impl ProvidersPlugin {
    fn new(
        state: Arc<Mutex<BTreeMap<String, ProviderResult>>>,
        settings: Arc<Mutex<ProviderSettings>>,
        providers: Arc<Vec<ProviderSpec>>,
        notifier: EntityNotifier,
    ) -> Self {
        Self {
            state,
            settings,
            providers,
            notifier,
        }
    }
}

#[async_trait::async_trait]
impl Plugin for ProvidersPlugin {
    fn get_entities(&self) -> Vec<Entity> {
        let settings = match self.settings.lock() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        };
        let mut entities = self
            .providers
            .iter()
            .map(|spec| provider_config_to_entity(spec, &settings))
            .collect::<Vec<_>>();
        drop(settings);

        let state = match self.state.lock() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        };
        entities.extend(state.values().filter_map(provider_result_to_entity));
        entities
    }

    async fn handle_action(
        &self,
        urn: Urn,
        action: String,
        params: serde_json::Value,
    ) -> anyhow::Result<serde_json::Value> {
        if urn.entity_type() != CONFIG_ENTITY_TYPE || action != "set-enabled" {
            anyhow::bail!("Unknown action: {action}");
        }

        let kind = supported_provider_kind(urn.id())
            .ok_or_else(|| anyhow::anyhow!("Unknown provider: {}", urn.id()))?;
        let enabled = params
            .get("enabled")
            .and_then(serde_json::Value::as_bool)
            .ok_or_else(|| anyhow::anyhow!("set-enabled requires a boolean enabled parameter"))?;

        {
            let mut settings = match self.settings.lock() {
                Ok(guard) => guard,
                Err(poisoned) => poisoned.into_inner(),
            };
            let previous = settings.enabled.get(kind.slug()).copied();
            settings.set_enabled(kind, enabled);
            if let Err(error) = settings.save() {
                match previous {
                    Some(value) => settings.enabled.insert(kind.slug().to_string(), value),
                    None => settings.enabled.remove(kind.slug()),
                };
                return Err(error);
            }
        }

        if !enabled {
            let mut state = match self.state.lock() {
                Ok(guard) => guard,
                Err(poisoned) => poisoned.into_inner(),
            };
            state.remove(kind.slug());
        }
        self.notifier.notify();
        Ok(serde_json::Value::Null)
    }
}

fn provider_config_to_entity(spec: &ProviderSpec, settings: &ProviderSettings) -> Entity {
    let config = ProviderConfig {
        provider: spec.kind.slug().to_string(),
        display_name: spec.kind.display_name().to_string(),
        enabled: settings.is_enabled(spec.kind),
        configured: spec.provider.auth_resolver().have_credentials(),
    };
    Entity::new(
        Urn::new("providers", CONFIG_ENTITY_TYPE, spec.kind.slug()),
        CONFIG_ENTITY_TYPE,
        &config,
    )
}

fn supported_provider_kind(slug: &str) -> Option<ProviderKind> {
    ProviderKind::all()
        .iter()
        .copied()
        .chain(std::iter::once(ProviderKind::Mimo))
        .find(|kind| kind.slug() == slug)
}

fn provider_result_to_entity(result: &ProviderResult) -> Option<Entity> {
    let ProviderStatus::Available { quota } = &result.status else {
        return None;
    };

    let windows: Vec<ProviderUsageWindow> = quota
        .windows
        .iter()
        // A quota with no reset boundary is not actionable usage data for
        // the overview. Do not publish it as a misleading limit card.
        .filter(|window| window.reset_at.is_some())
        .map(|window| ProviderUsageWindow {
            window_type: window.window_type.clone(),
            used: window.used,
            limit: window.limit,
            remaining: window.remaining,
            reset_at: window.reset_at.map(|time| time.timestamp_millis()),
            period_seconds: window.period_seconds,
        })
        .collect();
    if windows.is_empty() {
        return None;
    }

    let usage = ProviderUsage {
        provider: result.kind.slug().to_string(),
        display_name: result
            .card_title
            .clone()
            .unwrap_or_else(|| result.kind.display_name().to_string()),
        plan_name: quota.plan_name.clone(),
        unlimited: quota.unlimited,
        windows,
        fetched_at: result.fetched_at.timestamp_millis(),
        cached_at: result.cached_at.map(|time| time.timestamp_millis()),
    };

    Some(Entity::new(
        Urn::new("providers", ENTITY_TYPE, &result.cache_key()),
        ENTITY_TYPE,
        &usage,
    ))
}

fn parse_key_file(content: &str) -> Option<String> {
    for line in content.lines().map(str::trim) {
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if let Some(value) = line.strip_prefix("api_key=") {
            return Some(value.trim().trim_matches('"').to_string());
        }
        if let Some(value) = line.strip_prefix("token=") {
            return Some(value.trim().trim_matches('"').to_string());
        }
        if !line.contains('=') {
            return Some(line.to_string());
        }
    }
    None
}

fn home_paths(paths: &[&str]) -> Vec<PathBuf> {
    let home = dirs::home_dir().unwrap_or_default();
    paths.iter().map(|path| home.join(path)).collect()
}

fn env_auth(vars: &[(&'static str, &'static str)]) -> Box<dyn AuthResolver> {
    Box::new(EnvResolver::new(vars.to_vec()))
}

fn key_file_auth(paths: &[&str], source_name: &str) -> Box<dyn AuthResolver> {
    Box::new(FileResolver::new(
        home_paths(paths),
        parse_key_file,
        source_name,
    ))
}

fn cookie_file_auth(paths: &[&str], source_name: &str) -> Box<dyn AuthResolver> {
    Box::new(CookieFileResolver::new(home_paths(paths), source_name))
}

fn opencode_auth(slot: OpencodeSlot) -> Box<dyn AuthResolver> {
    Box::new(OpencodeAuthResolver::new(slot))
}

fn pi_auth(kind: ProviderKind) -> Box<dyn AuthResolver> {
    Box::new(PiAuthResolver::new(kind))
}

fn multi_auth(resolvers: Vec<Box<dyn AuthResolver>>) -> Box<dyn AuthResolver> {
    Box::new(MultiResolver::new(resolvers))
}

/// Build the same credential fallbacks used by the `quotas` CLI, without
/// importing its CLI configuration or writing credentials back to disk.
fn auth_for(kind: ProviderKind) -> Box<dyn AuthResolver> {
    match kind {
        ProviderKind::Claude => multi_auth(vec![
            Box::new(OAuthFileResolver::claude()),
            opencode_auth(OpencodeSlot::Anthropic),
            pi_auth(ProviderKind::Claude),
            env_auth(&[("ANTHROPIC_API_KEY", "anthropic")]),
        ]),
        ProviderKind::Codex => multi_auth(vec![
            Box::new(OAuthFileResolver::codex()),
            opencode_auth(OpencodeSlot::Openai),
            pi_auth(ProviderKind::Codex),
            env_auth(&[("OPENAI_API_KEY", "openai")]),
        ]),
        ProviderKind::Cursor => Box::new(CursorAuthResolver::new()),
        ProviderKind::DeepSeek => multi_auth(vec![
            env_auth(&[("DEEPSEEK_API_KEY", "deepseek")]),
            key_file_auth(&[".deepseek"], "deepseek"),
            pi_auth(ProviderKind::DeepSeek),
        ]),
        ProviderKind::Antigravity => multi_auth(vec![
            Box::new(OAuthFileResolver::antigravity()),
            env_auth(&[
                ("GEMINI_API_KEY", "antigravity"),
                ("GOOGLE_API_KEY", "antigravity"),
            ]),
            key_file_auth(&[".gemini-api-key", ".antigravity-api-key"], "antigravity"),
            pi_auth(ProviderKind::Antigravity),
        ]),
        ProviderKind::GitHubCopilot => multi_auth(vec![
            opencode_auth(OpencodeSlot::GitHubCopilot),
            pi_auth(ProviderKind::GitHubCopilot),
            env_auth(&[("GITHUB_COPILOT_TOKEN", "github_copilot")]),
        ]),
        ProviderKind::Grok => multi_auth(vec![
            Box::new(OAuthFileResolver::grok()),
            pi_auth(ProviderKind::Grok),
            env_auth(&[
                ("XAI_MANAGEMENT_KEY", "xai_management"),
                ("XAI_MGMT_KEY", "xai_mgmt"),
                ("GROK_MANAGEMENT_KEY", "grok_management"),
                ("XAI_API_KEY", "xai"),
                ("GROK_CODE_XAI_API_KEY", "grok_code"),
            ]),
            key_file_auth(
                &[
                    ".xai-management-key",
                    ".xai/management_key",
                    ".config/xai/management_key",
                ],
                "xai-management",
            ),
            key_file_auth(&[".xai", ".xai-api-key"], "xai"),
        ]),
        ProviderKind::Kimi => multi_auth(vec![
            env_auth(&[("MOONSHOT_API_KEY", "moonshot"), ("KIMI_API_KEY", "kimi")]),
            key_file_auth(&[".moonshot", ".kimi"], "kimi"),
            Box::new(KimiCliResolver::new()),
            opencode_auth(OpencodeSlot::Kimi),
            pi_auth(ProviderKind::Kimi),
        ]),
        ProviderKind::Minimax => multi_auth(vec![
            env_auth(&[("MINIMAX_API_KEY", "minimax")]),
            key_file_auth(&[".minimax"], "minimax"),
            opencode_auth(OpencodeSlot::Minimax),
            pi_auth(ProviderKind::Minimax),
        ]),
        ProviderKind::OpenRouter => multi_auth(vec![
            env_auth(&[("OPENROUTER_API_KEY", "openrouter")]),
            key_file_auth(&[".openrouter"], "openrouter"),
            pi_auth(ProviderKind::OpenRouter),
        ]),
        ProviderKind::SiliconFlow => multi_auth(vec![
            env_auth(&[
                ("SILICONFLOW_API_KEY", "siliconflow"),
                ("SILICON_FLOW_API_KEY", "siliconflow"),
            ]),
            key_file_auth(&[".siliconflow"], "siliconflow"),
            pi_auth(ProviderKind::SiliconFlow),
        ]),
        ProviderKind::Zai => multi_auth(vec![
            env_auth(&[("ZHIPU_API_KEY", "zhipu"), ("ZAI_API_KEY", "zai")]),
            key_file_auth(&[".api-zai"], "zai"),
            opencode_auth(OpencodeSlot::Zai),
            pi_auth(ProviderKind::Zai),
        ]),
        // Mimo is disabled by the quotas CLI's default list because its
        // platform endpoint commonly requires browser-cookie authentication.
        ProviderKind::Mimo => multi_auth(vec![
            cookie_file_auth(&[".config/mimo/cookie", ".mimo-cookie"], "mimo-cookie"),
            env_auth(&[
                ("XIAOMI_MIMO_API_KEY", "xiaomi_mimo"),
                ("XIAOMI_API_KEY", "xiaomi"),
                ("MIMO_API_KEY", "mimo"),
            ]),
            key_file_auth(&[".mimo-key", ".xiaomimimo", ".mimo"], "mimo"),
        ]),
    }
}

fn provider_for(kind: ProviderKind, auth: Box<dyn AuthResolver>) -> Box<dyn Provider> {
    match kind {
        ProviderKind::Claude => Box::new(quotas::providers::claude::ClaudeProvider::new(auth)),
        ProviderKind::Codex => Box::new(quotas::providers::codex::CodexProvider::new(auth)),
        ProviderKind::Cursor => Box::new(quotas::providers::cursor::CursorProvider::new(auth)),
        ProviderKind::DeepSeek => {
            Box::new(quotas::providers::deepseek::DeepSeekProvider::new(auth))
        }
        ProviderKind::Antigravity => Box::new(
            quotas::providers::antigravity::AntigravityProvider::new(auth),
        ),
        ProviderKind::GitHubCopilot => {
            Box::new(quotas::providers::github_copilot::GitHubCopilotProvider::new(auth))
        }
        ProviderKind::Grok => Box::new(quotas::providers::grok::GrokProvider::new(auth)),
        ProviderKind::Kimi => Box::new(quotas::providers::kimi::KimiProvider::new(auth)),
        ProviderKind::Minimax => Box::new(quotas::providers::minimax::MinimaxProvider::new(auth)),
        ProviderKind::OpenRouter => {
            Box::new(quotas::providers::openrouter::OpenRouterProvider::new(auth))
        }
        ProviderKind::SiliconFlow => Box::new(
            quotas::providers::siliconflow::SiliconFlowProvider::new(auth),
        ),
        ProviderKind::Zai => Box::new(quotas::providers::zai::ZaiProvider::new(auth)),
        ProviderKind::Mimo => Box::new(quotas::providers::mimo::MimoProvider::new(auth)),
    }
}

fn build_providers() -> Vec<ProviderSpec> {
    ProviderKind::all()
        .iter()
        .copied()
        .chain(std::iter::once(ProviderKind::Mimo))
        .map(|kind| ProviderSpec {
            kind,
            provider: provider_for(kind, auth_for(kind)),
        })
        .collect()
}

fn update_state(
    state: &Arc<Mutex<BTreeMap<String, ProviderResult>>>,
    providers: &[ProviderSpec],
    settings: &ProviderSettings,
    results: Vec<std::result::Result<ProviderResult, quotas::Error>>,
) -> bool {
    let mut state = match state.lock() {
        Ok(guard) => guard,
        Err(poisoned) => poisoned.into_inner(),
    };
    let mut changed = false;

    for spec in providers {
        if !settings.is_enabled(spec.kind) {
            changed |= state.remove(spec.kind.slug()).is_some();
        }
    }

    for result in results {
        match result {
            Ok(result) => {
                let key = result.cache_key();
                match &result.status {
                    ProviderStatus::Available { .. } => {
                        state.insert(key, result);
                        changed = true;
                    }
                    ProviderStatus::AuthRequired => {
                        changed |= state.remove(&key).is_some();
                    }
                    status => {
                        log::debug!(
                            "[providers] {} returned {:?}; retaining last good data",
                            result.kind.slug(),
                            status
                        );
                    }
                }
            }
            Err(error) => {
                log::debug!("[providers] quota fetch skipped or failed: {error}");
            }
        }
    }

    changed
}

fn main() -> Result<()> {
    PluginRunner::new("providers", &[ENTITY_TYPE, CONFIG_ENTITY_TYPE])
        .meta("AI Providers", "Usage quotas for configured AI providers")
        .run(|notifier| async move {
            let state = Arc::new(Mutex::new(BTreeMap::new()));
            let settings = Arc::new(Mutex::new(ProviderSettings::load()));
            let providers = Arc::new(build_providers());
            let task_state = state.clone();
            let task_settings = settings.clone();
            let task_providers = providers.clone();
            let task_notifier = notifier.clone();

            spawn_monitored("providers-poll", async move {
                loop {
                    let current_settings = match task_settings.lock() {
                        Ok(guard) => guard.clone(),
                        Err(poisoned) => poisoned.into_inner().clone(),
                    };
                    let fetches = task_providers
                        .iter()
                        .filter(|spec| {
                            current_settings.is_enabled(spec.kind)
                                && spec.provider.auth_resolver().have_credentials()
                        })
                        .map(|spec| spec.provider.fetch());
                    let results = join_all(fetches).await;

                    if update_state(&task_state, &task_providers, &current_settings, results) {
                        task_notifier.notify();
                    }

                    tokio::time::sleep(Duration::from_secs(POLL_INTERVAL_SECS)).await;
                }
            });

            Ok(ProvidersPlugin::new(state, settings, providers, notifier))
        })
}

#[cfg(test)]
mod tests {
    use super::{
        ProviderKind, ProviderResult, ProviderStatus, ProviderUsage, build_providers,
        provider_result_to_entity,
    };
    use chrono::Utc;
    use quotas::providers::{ProviderQuota, QuotaWindow};
    use waft_plugin::serde_json;

    #[test]
    fn builds_every_provider_in_the_library() {
        assert_eq!(build_providers().len(), ProviderKind::all().len() + 1);
    }

    #[test]
    fn maps_available_provider_windows_to_a_waft_entity() {
        let result = ProviderResult {
            kind: ProviderKind::Claude,
            status: ProviderStatus::Available {
                quota: ProviderQuota {
                    plan_name: "Max".to_string(),
                    unlimited: false,
                    banked_resets: None,
                    windows: vec![
                        QuotaWindow {
                            window_type: "weekly".to_string(),
                            used: 25,
                            limit: 100,
                            remaining: 75,
                            reset_at: Some(Utc::now()),
                            period_seconds: Some(604_800),
                        },
                        QuotaWindow {
                            window_type: "CRED_LIMIT".to_string(),
                            used: 1,
                            limit: 1,
                            remaining: 0,
                            reset_at: None,
                            period_seconds: None,
                        },
                    ],
                },
            },
            fetched_at: Utc::now(),
            raw_response: None,
            auth_source: None,
            cached_at: None,
            instance_key: None,
            profile_id: None,
            card_title: None,
        };

        let entity = provider_result_to_entity(&result).expect("available result should map");
        assert_eq!(entity.urn.as_str(), "providers/provider-usage/claude");
        let usage: ProviderUsage = serde_json::from_value(entity.data).expect("valid usage entity");
        assert_eq!(usage.provider, "claude");
        assert_eq!(usage.windows.len(), 1);
        assert_eq!(usage.windows[0].window_type, "weekly");
        assert_eq!(usage.windows[0].remaining, 75);
    }
}
