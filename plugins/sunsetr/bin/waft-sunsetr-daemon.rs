//! Sunsetr plugin — night light toggle.
//!
//! Provides a `night-light` entity via the sunsetr CLI tool.
//! Features:
//! - Toggle sunsetr on/off
//! - Display current period (day/night) and next transition time
//! - Preset selection
//! - Live event stream via `sunsetr S --json --follow`
//!
//! Configuration (in ~/.config/waft/config.toml):
//! ```toml
//! [[plugins]]
//! id = "sunsetr"
//! ```

use std::sync::OnceLock;

use anyhow::{Context, Result};
use log::{debug, info, warn};
use std::collections::HashMap;
use std::io::BufRead;
use std::process::Stdio;
use waft_i18n::I18n;

static I18N: OnceLock<I18n> = OnceLock::new();

fn i18n() -> &'static I18n {
    I18N.get_or_init(|| {
        I18n::new(&[
            ("en-US", include_str!("../locales/en-US/sunsetr.ftl")),
            ("cs-CZ", include_str!("../locales/cs-CZ/sunsetr.ftl")),
        ])
    })
}
use std::sync::{Arc, Mutex as StdMutex};
use std::time::Duration;
use sunsetr::config as sunsetr_config;
use waft_plugin::*;

const ERR_NO_PROCESS: &str = "no sunsetr process is running";

// ---------------------------------------------------------------------------
// State
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Default)]
struct SunsetrState {
    /// True if sunsetr process is running
    active: bool,
    /// Current period ("day", "night", or custom)
    period: Option<String>,
    /// Next transition time (HH:MM)
    next_transition: Option<String>,
    /// Available presets
    presets: Vec<String>,
    /// Currently active preset (None = default)
    active_preset: Option<String>,
}

// ---------------------------------------------------------------------------
// CLI IPC helpers
// ---------------------------------------------------------------------------

async fn run_sunsetr(args: &[&str]) -> Result<(i32, String, String)> {
    let output = tokio::process::Command::new("sunsetr")
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .await
        .map_err(|e| anyhow::anyhow!("Failed to run sunsetr {:?}: {e}", args))?;

    let code = output.status.code().unwrap_or(-1);
    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    Ok((code, stdout, stderr))
}

async fn run_ipc_variants(variants: &[&[&str]]) -> Result<()> {
    let mut errors: Vec<String> = Vec::new();

    for args in variants {
        let (code, _stdout, stderr) = run_sunsetr(args).await?;
        if code == 0 {
            return Ok(());
        }
        errors.push(format!("args={args:?} code={code} stderr={stderr}"));
    }

    anyhow::bail!("sunsetr command failed: {}", errors.join(" | "))
}

async fn ipc_start() -> Result<()> {
    run_ipc_variants(&[&["-b"], &["start"]]).await
}

async fn ipc_stop() -> Result<()> {
    run_ipc_variants(&[&["stop"], &["off"]]).await
}

#[derive(Debug, Clone, serde::Deserialize)]
struct SunsetrJsonEvent {
    period: Option<String>,
    next_period: Option<String>,
    to_period: Option<String>,
    active_preset: Option<String>,
}

fn is_no_process_error(s: &str) -> bool {
    s.to_lowercase().contains(ERR_NO_PROCESS)
}

/// Parse an RFC3339 timestamp into HH:MM.
fn hhmm_from_rfc3339(ts: &str) -> Option<String> {
    let t_pos = ts.find('T')?;
    let time = ts.get(t_pos + 1..)?;
    if time.len() < 5 {
        return None;
    }

    let hhmm = &time[..5];
    let bytes = hhmm.as_bytes();
    if bytes.len() != 5 {
        return None;
    }

    if !(bytes[0].is_ascii_digit()
        && bytes[1].is_ascii_digit()
        && bytes[2] == b':'
        && bytes[3].is_ascii_digit()
        && bytes[4].is_ascii_digit())
    {
        return None;
    }

    let hh = ((bytes[0] - b'0') as u32) * 10 + ((bytes[1] - b'0') as u32);
    let mm = ((bytes[3] - b'0') as u32) * 10 + ((bytes[4] - b'0') as u32);

    if hh < 24 && mm < 60 {
        Some(hhmm.to_string())
    } else {
        None
    }
}

/// Parse status JSON into state fields.
fn parse_status_event(ev: &SunsetrJsonEvent) -> (Option<String>, Option<String>) {
    let period = ev
        .period
        .as_deref()
        .or(ev.to_period.as_deref())
        .map(|s| s.to_string());

    let next_transition = ev.next_period.as_deref().and_then(hhmm_from_rfc3339);

    (period, next_transition)
}

async fn ipc_status_raw() -> Result<Option<SunsetrJsonEvent>> {
    let (code, stdout, stderr) = run_sunsetr(&["S", "--json"]).await?;

    if code != 0 {
        let combined_lc = format!("{stderr}\n{stdout}").to_lowercase();
        if combined_lc.contains(ERR_NO_PROCESS) {
            return Ok(None);
        }
        anyhow::bail!("sunsetr S --json failed (code {code}): {stderr}");
    }

    let json = match stdout.find('{') {
        Some(idx) => &stdout[idx..],
        None => {
            if stdout.to_lowercase().contains(ERR_NO_PROCESS) {
                return Ok(None);
            }
            anyhow::bail!("sunsetr S --json returned no JSON payload");
        }
    };

    let ev: SunsetrJsonEvent = serde_json::from_str(json)
        .map_err(|e| anyhow::anyhow!("Failed to parse sunsetr JSON status: {e}"))?;
    Ok(Some(ev))
}

async fn ipc_status() -> Result<(bool, Option<String>, Option<String>, Option<String>)> {
    match ipc_status_raw().await? {
        Some(ev) => {
            let (period, next_transition) = parse_status_event(&ev);
            let active_preset = ev.active_preset.as_ref().and_then(|p| {
                if p == "default" {
                    None
                } else {
                    Some(p.clone())
                }
            });
            Ok((true, period, next_transition, active_preset))
        }
        None => Ok((false, None, None, None)),
    }
}

async fn query_presets() -> Result<Vec<String>> {
    let (code, stdout, stderr) = run_sunsetr(&["preset", "list"]).await?;

    if code != 0 {
        anyhow::bail!("sunsetr preset list failed (code {code}): {stderr}");
    }

    let presets: Vec<String> = stdout
        .lines()
        .map(|line| line.trim())
        .filter(|line| !line.is_empty())
        .map(|line| line.to_string())
        .collect();

    Ok(presets)
}

async fn set_preset(preset_name: &str) -> Result<()> {
    let (code, _stdout, stderr) = run_sunsetr(&["preset", preset_name]).await?;

    if code != 0 {
        anyhow::bail!("sunsetr preset {preset_name} failed (code {code}): {stderr}");
    }

    Ok(())
}

/// Query all config fields from sunsetr via `sunsetr get --json all`.
async fn query_config(target: &str) -> Result<HashMap<String, String>> {
    let (code, stdout, stderr) =
        run_sunsetr(&["get", "--target", target, "--json", "all"]).await?;

    if code != 0 {
        anyhow::bail!("sunsetr get --json all failed (code {code}): {stderr}");
    }

    let json_start = match stdout.find('{') {
        Some(idx) => &stdout[idx..],
        None => {
            anyhow::bail!("sunsetr get --json all returned no JSON payload");
        }
    };

    let values: HashMap<String, serde_json::Value> = serde_json::from_str(json_start)
        .map_err(|e| anyhow::anyhow!("Failed to parse sunsetr config JSON: {e}"))?;

    // Convert all values to strings (sunsetr returns mixed types)
    let string_values: HashMap<String, String> = values
        .into_iter()
        .map(|(k, v)| {
            let s = match &v {
                serde_json::Value::String(s) => s.clone(),
                serde_json::Value::Number(n) => n.to_string(),
                serde_json::Value::Bool(b) => b.to_string(),
                other => other.to_string(),
            };
            (k, s)
        })
        .collect();

    Ok(string_values)
}

fn backoff_duration(attempt: usize) -> Duration {
    let ms = match attempt {
        0 => 50,
        1 => 75,
        2 => 100,
        3 => 125,
        4 => 150,
        5 => 200,
        6 => 250,
        _ => 300,
    };
    Duration::from_millis(ms)
}

/// Wait for sunsetr to become ready after starting it.
async fn refresh_after_start() -> Result<(bool, Option<String>, Option<String>, Option<String>)> {
    let mut last_err: Option<anyhow::Error> = None;

    for attempt in 0..20 {
        match ipc_status().await {
            Ok((true, period, next_transition, active_preset)) => {
                return Ok((true, period, next_transition, active_preset));
            }
            Ok((false, ..)) => {
                // not-ready, retry
            }
            Err(e) => {
                if !is_no_process_error(&e.to_string()) {
                    last_err = Some(e);
                }
            }
        }

        tokio::time::sleep(backoff_duration(attempt)).await;
    }

    if let Some(e) = last_err {
        return Err(e);
    }

    anyhow::bail!("sunsetr did not become ready in time");
}

// ---------------------------------------------------------------------------
// Plugin
// ---------------------------------------------------------------------------

struct SunsetrPlugin {
    state: Arc<StdMutex<SunsetrState>>,
    config_values: Arc<StdMutex<Option<HashMap<String, String>>>>,
    current_target: Arc<StdMutex<String>>,
}

impl SunsetrPlugin {
    fn new(
        state: Arc<StdMutex<SunsetrState>>,
        config_values: Arc<StdMutex<Option<HashMap<String, String>>>>,
        current_target: Arc<StdMutex<String>>,
    ) -> Self {
        Self {
            state,
            config_values,
            current_target,
        }
    }

    fn get_state(&self) -> SunsetrState {
        match self.state.lock() {
            Ok(g) => g.clone(),
            Err(e) => {
                warn!("[sunsetr] mutex poisoned, recovering: {e}");
                e.into_inner().clone()
            }
        }
    }

    fn get_target(&self) -> String {
        match self.current_target.lock() {
            Ok(g) => g.clone(),
            Err(e) => {
                warn!("[sunsetr] mutex poisoned, recovering: {e}");
                e.into_inner().clone()
            }
        }
    }

    fn config_entity(&self) -> Option<Entity> {
        let values = match self.config_values.lock() {
            Ok(g) => g.clone(),
            Err(e) => {
                warn!("[sunsetr] mutex poisoned, recovering: {e}");
                e.into_inner().clone()
            }
        };

        let values = values?;
        let target = self.get_target();
        let config_data = sunsetr_config::build_config_entity(&target, &values);

        Some(Entity::new(
            Urn::new(
                "sunsetr",
                entity::display::NIGHT_LIGHT_CONFIG_ENTITY_TYPE,
                &target,
            ),
            entity::display::NIGHT_LIGHT_CONFIG_ENTITY_TYPE,
            &config_data,
        ))
    }
}

#[async_trait::async_trait]
impl Plugin for SunsetrPlugin {
    fn get_entities(&self) -> Vec<Entity> {
        let state = self.get_state();
        let night_light = entity::display::NightLight {
            active: state.active,
            period: state.period,
            next_transition: state.next_transition,
            presets: state.presets,
            active_preset: state.active_preset,
        };
        let mut entities = vec![Entity::new(
            Urn::new(
                "sunsetr",
                entity::display::NIGHT_LIGHT_ENTITY_TYPE,
                "default",
            ),
            entity::display::NIGHT_LIGHT_ENTITY_TYPE,
            &night_light,
        )];

        if let Some(config_entity) = self.config_entity() {
            entities.push(config_entity);
        }

        entities
    }

    async fn handle_action(
        &self,
        _urn: Urn,
        action: String,
        params: serde_json::Value,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        match action.as_str() {
            "toggle" => {
                let currently_active = self.get_state().active;

                let result = if currently_active {
                    // Stop sunsetr
                    match ipc_stop().await {
                        Ok(()) => ipc_status().await,
                        Err(e) => Err(e),
                    }
                } else {
                    // Start sunsetr
                    match ipc_start().await {
                        Ok(()) => refresh_after_start().await,
                        Err(e) => Err(e),
                    }
                };

                // Update state from result
                let became_active = {
                    let mut state = match self.state.lock() {
                        Ok(g) => g,
                        Err(e) => {
                            warn!("[sunsetr] mutex poisoned, recovering: {e}");
                            e.into_inner()
                        }
                    };

                    match result {
                        Ok((active, period, next_transition, active_preset)) => {
                            state.active = active;
                            state.period = period;
                            state.next_transition = next_transition;
                            if let Some(preset) = active_preset {
                                state.active_preset = Some(preset);
                            }
                            active
                        }
                        Err(e) => {
                            log::error!("[sunsetr] toggle action failed: {e}");
                            return Err(e.into());
                        }
                    }
                };

                // Refresh presets when becoming active (lock dropped above)
                if became_active && let Ok(presets) = query_presets().await {
                    let mut state = match self.state.lock() {
                        Ok(g) => g,
                        Err(e) => {
                            warn!("[sunsetr] mutex poisoned, recovering: {e}");
                            e.into_inner()
                        }
                    };
                    state.presets = presets;
                }
            }
            "select_preset" => {
                let preset_name = params
                    .as_str()
                    .ok_or("select_preset requires string parameter")?
                    .to_string();

                debug!("[sunsetr] Selecting preset: {}", preset_name);

                if preset_name == "default" {
                    if let Err(e) = set_preset("default").await {
                        warn!("[sunsetr] preset switch to 'default' failed: {e}");
                        return Err(e.into());
                    } else {
                        let mut state = match self.state.lock() {
                            Ok(g) => g,
                            Err(e) => {
                                warn!("[sunsetr] mutex poisoned, recovering: {e}");
                                e.into_inner()
                            }
                        };
                        state.active_preset = None;
                    }
                } else if let Err(e) = set_preset(&preset_name).await {
                    warn!("[sunsetr] preset switch to '{}' failed: {e}", preset_name);
                    return Err(e.into());
                } else {
                    let mut state = match self.state.lock() {
                        Ok(g) => g,
                        Err(e) => {
                            warn!("[sunsetr] mutex poisoned, recovering: {e}");
                            e.into_inner()
                        }
                    };
                    state.active_preset = Some(preset_name);
                }
            }
            "update_config" => {
                let field = params
                    .get("field")
                    .and_then(|v| v.as_str())
                    .ok_or("update_config requires 'field' parameter")?
                    .to_string();
                let value = params
                    .get("value")
                    .and_then(|v| v.as_str())
                    .ok_or("update_config requires 'value' string parameter")?
                    .to_string();

                if !sunsetr_config::validate_field_name(&field) {
                    return Err(format!("Unknown config field: {field}").into());
                }

                let target = self.get_target();
                debug!("[sunsetr] Updating config: {field}={value} (target={target})");

                let arg = format!("{field}={value}");
                let (code, _stdout, stderr) =
                    run_sunsetr(&["set", "--target", &target, &arg]).await?;

                if code != 0 {
                    return Err(
                        format!("sunsetr set failed (code {code}): {stderr}").into()
                    );
                }

                // Re-query config to capture side effects
                match query_config(&target).await {
                    Ok(values) => {
                        match self.config_values.lock() {
                            Ok(mut g) => *g = Some(values),
                            Err(e) => {
                                warn!("[sunsetr] mutex poisoned, recovering: {e}");
                                *e.into_inner() = Some(values);
                            }
                        }
                    }
                    Err(e) => {
                        warn!("[sunsetr] Failed to re-query config after update: {e}");
                    }
                }
            }
            "load_preset" => {
                let preset_name = params
                    .get("name")
                    .and_then(|v| v.as_str())
                    .ok_or("load_preset requires 'name' parameter")?
                    .to_string();

                debug!("[sunsetr] Loading preset config: {preset_name}");

                // Query the preset's config
                match query_config(&preset_name).await {
                    Ok(values) => {
                        match self.current_target.lock() {
                            Ok(mut g) => *g = preset_name.clone(),
                            Err(e) => {
                                warn!("[sunsetr] mutex poisoned, recovering: {e}");
                                *e.into_inner() = preset_name;
                            }
                        }
                        match self.config_values.lock() {
                            Ok(mut g) => *g = Some(values),
                            Err(e) => {
                                warn!("[sunsetr] mutex poisoned, recovering: {e}");
                                *e.into_inner() = Some(values);
                            }
                        }
                    }
                    Err(e) => {
                        return Err(
                            format!("Failed to load preset config: {e}").into()
                        );
                    }
                }
            }
            "create_preset" => {
                let name = params
                    .get("name")
                    .and_then(|v| v.as_str())
                    .ok_or("create_preset requires 'name' parameter")?
                    .to_string();

                if name.is_empty() || name == "default" || name.contains(' ') {
                    return Err("Invalid preset name".into());
                }

                debug!("[sunsetr] Creating preset: {name}");

                let (code, _stdout, stderr) =
                    run_sunsetr(&["preset", &name]).await?;

                if code != 0 {
                    return Err(
                        format!("Failed to create preset (code {code}): {stderr}").into()
                    );
                }

                // Refresh presets list
                if let Ok(presets) = query_presets().await {
                    let mut state = match self.state.lock() {
                        Ok(g) => g,
                        Err(e) => {
                            warn!("[sunsetr] mutex poisoned, recovering: {e}");
                            e.into_inner()
                        }
                    };
                    state.presets = presets;
                }

                // Query the new preset's config
                if let Ok(values) = query_config(&name).await {
                    match self.current_target.lock() {
                        Ok(mut g) => *g = name,
                        Err(e) => {
                            warn!("[sunsetr] mutex poisoned, recovering: {e}");
                            *e.into_inner() = name;
                        }
                    }
                    match self.config_values.lock() {
                        Ok(mut g) => *g = Some(values),
                        Err(e) => {
                            warn!("[sunsetr] mutex poisoned, recovering: {e}");
                            *e.into_inner() = Some(values);
                        }
                    }
                }
            }
            "delete_preset" => {
                let name = params
                    .get("name")
                    .and_then(|v| v.as_str())
                    .ok_or("delete_preset requires 'name' parameter")?
                    .to_string();

                if name == "default" {
                    return Err("Cannot delete default preset".into());
                }

                debug!("[sunsetr] Deleting preset: {name}");

                let preset_path = dirs::config_dir()
                    .ok_or("No config directory")?
                    .join(format!("sunsetr/presets/{name}.toml"));

                if preset_path.exists() {
                    std::fs::remove_file(&preset_path).map_err(|e| {
                        format!("Failed to delete preset file: {e}")
                    })?;
                }

                // Switch to default
                match self.current_target.lock() {
                    Ok(mut g) => *g = "default".to_string(),
                    Err(e) => {
                        warn!("[sunsetr] mutex poisoned, recovering: {e}");
                        *e.into_inner() = "default".to_string();
                    }
                }

                // Refresh presets list
                if let Ok(presets) = query_presets().await {
                    let mut state = match self.state.lock() {
                        Ok(g) => g,
                        Err(e) => {
                            warn!("[sunsetr] mutex poisoned, recovering: {e}");
                            e.into_inner()
                        }
                    };
                    state.presets = presets;
                }

                // Query default config
                if let Ok(values) = query_config("default").await {
                    match self.config_values.lock() {
                        Ok(mut g) => *g = Some(values),
                        Err(e) => {
                            warn!("[sunsetr] mutex poisoned, recovering: {e}");
                            *e.into_inner() = Some(values);
                        }
                    }
                }
            }
            _ => {}
        }

        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Follow task -- live event stream from sunsetr
// ---------------------------------------------------------------------------

fn spawn_follow_task(state: Arc<StdMutex<SunsetrState>>, notifier: EntityNotifier) {
    std::thread::spawn(move || {
        let mut child = match std::process::Command::new("sunsetr")
            .args(["S", "--json", "--follow"])
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
        {
            Ok(c) => c,
            Err(e) => {
                warn!("[sunsetr] follow spawn failed: {e}");
                return;
            }
        };

        let stdout = match child.stdout.take() {
            Some(s) => s,
            None => {
                warn!("[sunsetr] follow stdout missing");
                let _ = child.wait();
                return;
            }
        };

        let reader = std::io::BufReader::new(stdout);
        for line_result in reader.lines() {
            let line = match line_result {
                Ok(l) => l,
                Err(e) => {
                    debug!("[sunsetr] follow read error: {e}");
                    break;
                }
            };

            let ev: SunsetrJsonEvent = match serde_json::from_str(&line) {
                Ok(e) => e,
                Err(e) => {
                    debug!("[sunsetr] follow parse error: {e}");
                    continue;
                }
            };

            let (period, next_transition) = parse_status_event(&ev);

            let mut state_guard = match state.lock() {
                Ok(g) => g,
                Err(e) => {
                    warn!("[sunsetr] follow mutex poisoned, recovering: {e}");
                    e.into_inner()
                }
            };

            let changed = state_guard.period != period
                || state_guard.next_transition != next_transition
                || state_guard.active_preset != ev.active_preset;

            if changed {
                state_guard.period = period;
                state_guard.next_transition = next_transition;
                if let Some(preset) = &ev.active_preset {
                    if preset != "default" {
                        state_guard.active_preset = Some(preset.clone());
                    } else {
                        state_guard.active_preset = None;
                    }
                }

                drop(state_guard);
                notifier.notify();
            }
        }

        let _ = child.wait();
        debug!("[sunsetr] follow task stopped");
    });
}

fn binary_available() -> bool {
    match std::process::Command::new("sunsetr")
        .arg("--version")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
    {
        Ok(_) => true,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => false,
        Err(_) => true, // binary exists but failed for other reason
    }
}

fn main() -> Result<()> {
    // Handle `provides` CLI command before starting runtime
    if waft_plugin::manifest::handle_provides_i18n(
        &[
            entity::display::NIGHT_LIGHT_ENTITY_TYPE,
            entity::display::NIGHT_LIGHT_CONFIG_ENTITY_TYPE,
        ],
        i18n(),
        "plugin-name",
        "plugin-description",
    ) {
        return Ok(());
    }

    // Initialize logging
    waft_plugin::init_plugin_logger("info");

    info!("Starting sunsetr plugin...");

    if !binary_available() {
        info!("[sunsetr] sunsetr binary not found, exiting");
        return Ok(());
    }

    let rt = tokio::runtime::Runtime::new().context("failed to create tokio runtime")?;
    rt.block_on(async {
        // Query initial status
        let (active, period, next_transition, active_preset) =
            ipc_status().await.unwrap_or((false, None, None, None));
        info!("[sunsetr] Initial status: active={active}");

        // Query presets if active
        let presets = if active {
            query_presets().await.unwrap_or_default()
        } else {
            Vec::new()
        };

        // Query config if active
        let config_values = if active {
            match query_config("default").await {
                Ok(vals) => Some(vals),
                Err(e) => {
                    warn!("[sunsetr] Failed to query initial config: {e}");
                    None
                }
            }
        } else {
            None
        };

        let state = Arc::new(StdMutex::new(SunsetrState {
            active,
            period,
            next_transition,
            presets,
            active_preset,
        }));

        let config_values = Arc::new(StdMutex::new(config_values));
        let current_target = Arc::new(StdMutex::new("default".to_string()));

        let plugin = SunsetrPlugin::new(state.clone(), config_values, current_target);
        let (runtime, notifier) = PluginRuntime::new("sunsetr", plugin);

        // Spawn follow task to monitor sunsetr events.
        // Clone the notifier: the follow task's clone will be dropped when sunsetr
        // stops, but the original keeps the watch channel open so the runtime
        // doesn't interpret a closed channel as a shutdown signal.
        spawn_follow_task(state, notifier.clone());

        // Keep `notifier` alive for the duration of the runtime. When the user
        // toggles sunsetr off, the follow subprocess exits and drops its clone,
        // but the runtime continues because this reference still holds the
        // watch::Sender open.
        let _notifier = notifier;

        runtime.run().await?;
        Ok(())
    })
}
