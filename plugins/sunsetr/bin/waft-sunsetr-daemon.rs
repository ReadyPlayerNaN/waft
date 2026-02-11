//! Sunsetr daemon -- night light toggle.
//!
//! Controls the sunsetr CLI tool to manage screen color temperature.
//! Features:
//! - Toggle sunsetr on/off
//! - Display current period (day/night) and next transition time
//! - Preset selection via expandable menu
//! - Live event stream via `sunsetr S --json --follow`

use anyhow::Result;
use log::{debug, info, warn};
use std::io::BufRead;
use std::process::Stdio;
use std::sync::{Arc, Mutex as StdMutex};
use std::time::Duration;
use waft_plugin_sdk::*;

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
    busy: bool,
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
// Daemon
// ---------------------------------------------------------------------------

struct SunsetrDaemon {
    state: Arc<StdMutex<SunsetrState>>,
}

impl SunsetrDaemon {
    fn new(state: Arc<StdMutex<SunsetrState>>) -> Self {
        Self { state }
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

    fn build_details(state: &SunsetrState) -> Option<String> {
        if !state.active {
            return None;
        }

        state.next_transition.as_ref().map(|time| {
            let is_night = state
                .period
                .as_ref()
                .map(|p| !p.eq_ignore_ascii_case("day"))
                .unwrap_or(false);

            let key = if is_night { "Night until" } else { "Day until" };
            format!("{} {}", key, time)
        })
    }

    fn build_preset_menu(state: &SunsetrState) -> Widget {
        let mut children: Vec<Widget> = Vec::new();

        // "Default" row
        children.push(
            MenuRowBuilder::new("Default")
                .icon("preferences-system-symbolic")
                .trailing(Widget::Checkmark {
                    visible: state.active_preset.is_none(),
                })
                .on_click_action(Action {
                    id: "select_preset".into(),
                    params: ActionParams::String("default".into()),
                })
                .build(),
        );

        // Individual presets
        for preset in &state.presets {
            let is_active = state.active_preset.as_ref() == Some(preset);
            children.push(
                MenuRowBuilder::new(preset.as_str())
                    .icon("preferences-system-symbolic")
                    .trailing(Widget::Checkmark { visible: is_active })
                    .on_click_action(Action {
                        id: "select_preset".into(),
                        params: ActionParams::String(preset.clone()),
                    })
                    .build(),
            );
        }

        ColBuilder::new()
            .children(children)
            .build()
    }

    fn build_toggle(state: &SunsetrState) -> Widget {
        let mut builder = FeatureToggleBuilder::new("Night Light")
            .icon("night-light-symbolic")
            .active(state.active)
            .busy(state.busy)
            .on_toggle("toggle");

        if let Some(details) = Self::build_details(state) {
            builder = builder.details(details);
        }

        // Show expanded preset menu when active and presets exist
        if state.active && !state.presets.is_empty() {
            builder = builder.expanded_content(Self::build_preset_menu(state));
        }

        builder.build()
    }
}

#[async_trait::async_trait]
impl PluginDaemon for SunsetrDaemon {
    fn get_widgets(&self) -> Vec<NamedWidget> {
        let state = self.get_state();
        vec![NamedWidget {
            id: "sunsetr:toggle".to_string(),
            weight: 200,
            widget: Self::build_toggle(&state),
        }]
    }

    async fn handle_action(
        &self,
        _widget_id: String,
        action: Action,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        match action.id.as_str() {
            "toggle" => {
                let currently_active = self.get_state().active;

                // Set busy
                {
                    let mut state = match self.state.lock() {
                        Ok(g) => g,
                        Err(e) => {
                            warn!("[sunsetr] mutex poisoned, recovering: {e}");
                            e.into_inner()
                        }
                    };
                    state.busy = true;
                }

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
                    state.busy = false;

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
                            false
                        }
                    }
                };

                // Refresh presets when becoming active (lock dropped above)
                if became_active {
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
                }
            }
            "select_preset" => {
                let preset_name = match &action.params {
                    ActionParams::String(s) => s.clone(),
                    _ => return Ok(()),
                };

                debug!("[sunsetr] Selecting preset: {}", preset_name);

                if preset_name == "default" {
                    if let Err(e) = set_preset("default").await {
                        warn!("[sunsetr] preset switch to 'default' failed: {e}");
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
            _ => {}
        }

        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Follow task -- live event stream from sunsetr
// ---------------------------------------------------------------------------

fn spawn_follow_task(state: Arc<StdMutex<SunsetrState>>, notifier: WidgetNotifier) {
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
            None => return,
        };

        let reader = std::io::BufReader::new(stdout);
        for line in reader.lines() {
            let line = match line {
                Ok(l) => l,
                Err(_) => break,
            };
            let line = line.trim();
            if line.is_empty() {
                continue;
            }

            if is_no_process_error(line) || line.to_lowercase().starts_with("[error]") {
                let mut s = match state.lock() {
                    Ok(g) => g,
                    Err(e) => {
                        warn!("[sunsetr] mutex poisoned, recovering: {e}");
                        e.into_inner()
                    }
                };
                s.active = false;
                s.period = None;
                s.next_transition = None;
                drop(s);
                notifier.notify();
                continue;
            }

            match serde_json::from_str::<SunsetrJsonEvent>(line) {
                Ok(ev) => {
                    let (period, next_transition) = parse_status_event(&ev);
                    let active_preset = ev.active_preset.as_ref().and_then(|p| {
                        if p == "default" {
                            None
                        } else {
                            Some(p.clone())
                        }
                    });

                    let mut s = match state.lock() {
                        Ok(g) => g,
                        Err(e) => {
                            warn!("[sunsetr] mutex poisoned, recovering: {e}");
                            e.into_inner()
                        }
                    };
                    s.active = true;
                    s.period = period;
                    s.next_transition = next_transition;
                    if ev.active_preset.is_some() {
                        s.active_preset = active_preset;
                    }
                    drop(s);
                    notifier.notify();
                }
                Err(_) => {}
            }
        }

        // Follow stream ended -- mark inactive
        {
            let mut s = match state.lock() {
                Ok(g) => g,
                Err(e) => {
                    warn!("[sunsetr] mutex poisoned, recovering: {e}");
                    e.into_inner()
                }
            };
            s.active = false;
            s.period = None;
            s.next_transition = None;
        }
        notifier.notify();
        warn!("[sunsetr] follow task exited");

        let _ = child.kill();
        let _ = child.wait();
    });
}

// ---------------------------------------------------------------------------
// Main
// ---------------------------------------------------------------------------

#[tokio::main]
async fn main() -> Result<()> {
    waft_plugin_sdk::init_daemon_logger("info");
    info!("Starting sunsetr daemon...");

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

    let state = Arc::new(StdMutex::new(SunsetrState {
        active,
        period,
        next_transition,
        busy: false,
        presets,
        active_preset,
    }));

    let daemon = SunsetrDaemon::new(state.clone());
    let (server, notifier) = PluginServer::new("sunsetr-daemon", daemon);

    // Start live event stream
    spawn_follow_task(state, notifier);

    server.run().await?;

    Ok(())
}
