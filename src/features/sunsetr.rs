use crate::plugins::{FeatureToggle, Plugin};
use crate::ui::UiEvent;
use crate::ui::features::FeatureSpec;
use anyhow::{Context, Result};
use async_trait::async_trait;
use serde::Deserialize;
use std::process::Stdio;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;
use tokio::sync::mpsc;

const FEATURE_KEY: &str = "plugin::sunsetr";
const FEATURE_TITLE: &str = "Night light";
const FEATURE_ICON: &str = "night-light-symbolic";

/// `sunsetr` prints this (or similar) when the daemon isn't running.
/// Treat it as a normal "inactive" state.
const ERR_NO_PROCESS: &str = "no sunsetr process is running";

/// The toggle UI is about enabling/disabling night-light effects, not about daemon liveness.
/// If sunsetr reports "period == day", the effect is not active.
const PERIOD_DAY: &str = "day";

/// A minimal controller that can be captured by `'static` async closures.
///
/// Keep state in atomics, and reflect changes via the UI event bus.
/// This avoids keeping references into UI models/widgets.
#[derive(Clone)]
struct SunsetrController {
    active: Arc<AtomicBool>,
    ui_event_tx: Option<mpsc::UnboundedSender<UiEvent>>,
}

impl SunsetrController {
    fn apply(&self, status: SunsetrStatus) {
        self.apply_active(status.active);
        self.apply_status_text(status.next_transition_text.unwrap_or_default());
    }

    fn apply_active(&self, active: bool) {
        self.active.store(active, Ordering::Relaxed);

        if let Some(tx) = self.ui_event_tx.as_ref() {
            let _ = tx.send(UiEvent::FeatureActiveChanged {
                key: FEATURE_KEY.to_string(),
                active,
            });
        }
    }

    fn apply_status_text(&self, text: String) {
        if let Some(tx) = self.ui_event_tx.as_ref() {
            let _ = tx.send(UiEvent::FeatureStatusTextChanged {
                key: FEATURE_KEY.to_string(),
                text,
            });
        }
    }

    fn apply_inactive(&self) {
        self.apply_active(false);
        self.apply_status_text(String::new());
    }
}

/// `SunsetrPlugin` controls and displays `sunsetr` state.
///
/// `sunsetr` control happens via CLI IPC:
/// - `sunsetr S --json` -> snapshot JSON (when running)
/// - `sunsetr S --json --follow` -> newline-delimited JSON events (best-effort)
/// - `sunsetr -b` / `sunsetr start` -> start daemon (variant fallback)
/// - `sunsetr stop` / `sunsetr off` -> stop daemon (variant fallback)
pub struct SunsetrPlugin {
    initialized: bool,
    toggle: Option<FeatureSpec>,
    ui_event_tx: Option<mpsc::UnboundedSender<UiEvent>>,

    ctl_active: Arc<AtomicBool>,
}

impl SunsetrPlugin {
    pub fn new() -> Self {
        Self {
            initialized: false,
            toggle: None,
            ui_event_tx: None,
            ctl_active: Arc::new(AtomicBool::new(false)),
        }
    }

    /// Inject a UI event sender that will be used to notify the UI of state changes.
    pub fn with_ui_event_sender(mut self, tx: mpsc::UnboundedSender<UiEvent>) -> Self {
        self.ui_event_tx = Some(tx);
        self
    }

    fn controller(&self) -> SunsetrController {
        SunsetrController {
            active: self.ctl_active.clone(),
            ui_event_tx: self.ui_event_tx.clone(),
        }
    }

    fn create_feature_toggle_el(&mut self) {
        let enabled_flag = self.ctl_active.load(Ordering::Relaxed);
        let ctl = self.controller();

        let el = FeatureSpec::contentless_with_toggle(
            FEATURE_KEY,
            FEATURE_TITLE.to_string(),
            FEATURE_ICON.to_string(),
            enabled_flag,
            move |_key: &'static str, current_active: bool| {
                let ctl = ctl.clone();
                async move {
                    // Toggle semantics:
                    // - If currently active, request stop (daemon off) and refresh.
                    // - If currently inactive, request start (daemon on) and wait until status is available.
                    let desired_active = !current_active;

                    if desired_active {
                        if let Err(e) = SunsetrPlugin::ipc_start().await {
                            eprintln!("sunsetr start failed: {e}");
                        }

                        match SunsetrPlugin::refresh_after_start(ctl.clone()).await {
                            Ok(()) => {}
                            Err(e) => {
                                eprintln!("sunsetr refresh-after-start failed: {e}");
                                // If we can't confirm, keep UI optimistic (user asked to enable).
                                ctl.apply_active(true);
                                ctl.apply_status_text(String::new());
                            }
                        }

                        return;
                    }

                    if let Err(e) = SunsetrPlugin::ipc_stop().await {
                        eprintln!("sunsetr stop failed: {e}");
                    }

                    // One refresh pass: if sunsetr is stopped, status should be inactive.
                    match SunsetrPlugin::ipc_status().await {
                        Ok(status) => ctl.apply(status),
                        Err(e) => {
                            eprintln!("sunsetr status refresh failed: {e}");
                            ctl.apply_inactive();
                        }
                    }
                }
            },
        );

        self.toggle = Some(el);
    }

    fn feature_toggle_el(&self) -> &FeatureSpec {
        self.toggle
            .as_ref()
            .expect("SunsetrPlugin toggle not initialized")
    }

    fn feature_toggle(&self) -> FeatureToggle {
        FeatureToggle {
            id: FEATURE_KEY.to_string(),
            el: self.feature_toggle_el().clone(),
            weight: 11,
        }
    }

    async fn start_polling(&mut self) {
        // Backstop polling (in case follow-mode isn't available / fails).
        let ctl = self.controller();

        tokio::spawn(async move {
            let mut interval = tokio::time::interval(std::time::Duration::from_secs(20));
            loop {
                interval.tick().await;

                match SunsetrPlugin::ipc_status().await {
                    Ok(status) => ctl.apply(status),
                    Err(e) => {
                        // Unexpected errors should be visible, but "not running" is normal and should be quiet.
                        if !is_no_process_error(&e.to_string()) {
                            eprintln!("sunsetr status poll failed: {e}");
                        }
                        ctl.apply_inactive();
                    }
                }
            }
        });
    }

    async fn start_following(&mut self) {
        // Best-effort live event stream via `sunsetr S --json --follow` (newline-delimited JSON).
        // When sunsetr isn't running, this command can emit human-readable errors; ignore those quietly.
        let ctl = self.controller();

        tokio::spawn(async move {
            let mut child = match Command::new("sunsetr")
                .args(["S", "--json", "--follow"])
                .stdin(Stdio::null())
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .spawn()
            {
                Ok(c) => c,
                Err(e) => {
                    eprintln!("sunsetr follow spawn failed: {e}");
                    return;
                }
            };

            let stdout = match child.stdout.take() {
                Some(s) => s,
                None => return,
            };

            let mut lines = BufReader::new(stdout).lines();

            while let Ok(Some(line)) = lines.next_line().await {
                let line = line.trim();
                if line.is_empty() {
                    continue;
                }

                if is_no_process_error(line) || line.to_lowercase().starts_with("[error]") {
                    ctl.apply_inactive();
                    continue;
                }

                match serde_json::from_str::<SunsetrJsonEvent>(line) {
                    Ok(ev) => ctl.apply(SunsetrStatus::from_event(ev)),
                    Err(e) => {
                        // Sunsetr may interleave other text; ignore quietly unless it looks unrelated.
                        if !is_no_process_error(line) && !line.to_lowercase().starts_with("[error]")
                        {
                            eprintln!("sunsetr follow JSON parse failed: {e} (line: {line})");
                        }
                    }
                }
            }
        });
    }

    async fn refresh_after_start(ctl: SunsetrController) -> Result<()> {
        // `sunsetr -b` returns before daemon is ready; wait until status JSON is available.
        let mut last_err: Option<anyhow::Error> = None;

        for attempt in 0..20 {
            match Self::ipc_status().await {
                Ok(status) => {
                    // If status call succeeds, we have a coherent snapshot.
                    ctl.apply(status);
                    return Ok(());
                }
                Err(e) => {
                    // If it is still "no process", treat as not-ready and retry silently.
                    if is_no_process_error(&e.to_string()) {
                        // keep retrying
                    } else {
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

    async fn run_sunsetr(args: &[&str]) -> Result<(i32, String, String)> {
        let mut cmd = Command::new("sunsetr");
        cmd.args(args)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        let out = cmd
            .output()
            .await
            .with_context(|| format!("Failed to run sunsetr {:?}", args))?;

        let code = out.status.code().unwrap_or(-1);
        let stdout = String::from_utf8_lossy(&out.stdout).to_string();
        let stderr = String::from_utf8_lossy(&out.stderr).to_string();
        Ok((code, stdout, stderr))
    }

    async fn ipc_start() -> Result<()> {
        // Prefer `-b` (background); fall back to `start`.
        run_ipc_variants(&[&["-b"], &["start"]]).await
    }

    async fn ipc_stop() -> Result<()> {
        // CLI variants differ; try stop/off.
        run_ipc_variants(&[&["stop"], &["off"]]).await
    }

    async fn ipc_status() -> Result<SunsetrStatus> {
        match Self::ipc_status_raw().await? {
            Some(ev) => Ok(SunsetrStatus::from_event(ev)),
            None => Ok(SunsetrStatus::inactive()),
        }
    }

    /// Returns:
    /// - `Ok(Some(event))` when sunsetr is running and returned valid JSON
    /// - `Ok(None)` when sunsetr is NOT running (the normal "no process running" case)
    /// - `Err(_)` for other failures (missing binary, unexpected output, etc)
    async fn ipc_status_raw() -> Result<Option<SunsetrJsonEvent>> {
        let (code, stdout, stderr) = Self::run_sunsetr(&["S", "--json"]).await?;

        if code != 0 {
            let combined_lc = format!("{stderr}\n{stdout}").to_lowercase();
            if combined_lc.contains(ERR_NO_PROCESS) {
                return Ok(None);
            }
            anyhow::bail!("sunsetr S --json failed (code {code}): {stderr}");
        }

        // Sometimes tools print prefixes; trim to the first '{' just in case.
        let json = match stdout.find('{') {
            Some(idx) => &stdout[idx..],
            None => {
                if stdout.to_lowercase().contains(ERR_NO_PROCESS) {
                    return Ok(None);
                }
                anyhow::bail!("sunsetr S --json returned no JSON payload");
            }
        };

        let ev: SunsetrJsonEvent =
            serde_json::from_str(json).context("Failed to parse sunsetr JSON status")?;
        Ok(Some(ev))
    }
}

#[derive(Debug, Clone)]
struct SunsetrStatus {
    /// Whether the night-light effect is currently active (i.e. non-day period).
    active: bool,

    /// Next transition time as HH:MM to show in the tile's status text.
    next_transition_text: Option<String>,
}

impl SunsetrStatus {
    fn inactive() -> Self {
        Self {
            active: false,
            next_transition_text: None,
        }
    }

    fn from_event(ev: SunsetrJsonEvent) -> Self {
        let period = ev
            .period
            .as_deref()
            .or_else(|| ev.to_period.as_deref())
            .unwrap_or(PERIOD_DAY);

        let active = !period.eq_ignore_ascii_case(PERIOD_DAY);

        let next_transition_text = ev.next_period.as_deref().and_then(hhmm_from_rfc3339);

        Self {
            active,
            next_transition_text,
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
struct SunsetrJsonEvent {
    // Snapshot fields (from `sunsetr S --json`)
    period: Option<String>,
    next_period: Option<String>,

    // Some follow-mode events include extra fields (like `event_type`), but we don't need them.
    // Some events use `to_period`, but state_applied uses `period`.
    to_period: Option<String>,
}

async fn run_ipc_variants(variants: &[&[&str]]) -> Result<()> {
    let mut errors: Vec<String> = Vec::new();

    for args in variants {
        let (code, _stdout, stderr) = SunsetrPlugin::run_sunsetr(args).await?;
        if code == 0 {
            return Ok(());
        }
        errors.push(format!("args={args:?} code={code} stderr={stderr}"));
    }

    anyhow::bail!("sunsetr command failed: {}", errors.join(" | "))
}

fn is_no_process_error(s: &str) -> bool {
    s.to_lowercase().contains(ERR_NO_PROCESS)
}

fn backoff_duration(attempt: usize) -> std::time::Duration {
    // Fast early, then cap. Deterministic + tiny (no extra deps).
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
    std::time::Duration::from_millis(ms)
}

fn hhmm_from_rfc3339(ts: &str) -> Option<String> {
    // sunsetr uses RFC3339 timestamps like: 2026-01-06T07:47:33.000691838+01:00
    // We only need HH:MM; extracting it is safe and avoids extra time/date deps.
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

#[async_trait(?Send)]
impl Plugin for SunsetrPlugin {
    fn name(&self) -> &str {
        FEATURE_KEY
    }

    async fn initialize(&mut self) -> Result<()> {
        if self.initialized {
            return Ok(());
        }

        self.create_feature_toggle_el();

        // Fetch initial state; don't fail the whole app if sunsetr isn't installed/available.
        let initial = match Self::ipc_status().await {
            Ok(s) => s,
            Err(e) => {
                if !is_no_process_error(&e.to_string()) {
                    eprintln!("sunsetr not available (status failed): {e}");
                }
                SunsetrStatus::inactive()
            }
        };

        self.controller().apply(initial);

        // Always start polling; follow is best-effort.
        self.start_polling().await;
        self.start_following().await;

        self.initialized = true;
        Ok(())
    }

    async fn cleanup(&mut self) -> Result<()> {
        self.initialized = false;
        Ok(())
    }

    fn feature_toggles(&self) -> Vec<FeatureToggle> {
        vec![self.feature_toggle()]
    }
}
