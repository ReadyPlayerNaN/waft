use anyhow::Result;
use flume::Sender;
use log::warn;
use std::process::Stdio;
use std::time::Duration;
use tokio::io::AsyncBufReadExt;

use super::values::Status;

const ERR_NO_PROCESS: &str = "no sunsetr process is running";

#[derive(Debug, Clone)]
pub enum SunsetrIpcEvents {
    Busy(bool),
    Status(Status),
    Error(String),
}

pub async fn spawn_start(sender: Sender<SunsetrIpcEvents>) -> Result<()> {
    sender.send(SunsetrIpcEvents::Busy(true))?;

    if let Err(e) = ipc_start().await {
        sender.send(SunsetrIpcEvents::Error(format!(
            "sunsetr start failed: {e}"
        )))?;
        return Err(e);
    }

    match refresh_after_start().await {
        Ok(status) => sender.send(SunsetrIpcEvents::Status(status))?,
        Err(e) => sender.send(SunsetrIpcEvents::Error(format!(
            "sunsetr refresh-after-start failed: {e}"
        )))?,
    }
    Ok(())
}

pub async fn spawn_stop(sender: Sender<SunsetrIpcEvents>) -> Result<()> {
    let _ = sender.send(SunsetrIpcEvents::Busy(true));
    if let Err(e) = ipc_stop().await {
        sender.send(SunsetrIpcEvents::Error(format!("sunsetr stop failed: {e}")))?;
        return Err(e);
    }

    // One refresh pass: if sunsetr is stopped, status should be inactive.
    match ipc_status().await {
        Ok(status) => sender.send(SunsetrIpcEvents::Status(status))?,
        Err(e) => sender.send(SunsetrIpcEvents::Error(format!(
            "sunsetr status refresh failed: {e}"
        )))?,
    };
    Ok(())
}

pub fn spawn_following(sender: Sender<SunsetrIpcEvents>) -> Result<()> {
    tokio::spawn(async move {
        // Best-effort live event stream via `sunsetr S --json --follow`.
        // We keep this robust: parse only valid JSON lines, ignore known non-json errors.
        let mut child = match tokio::process::Command::new("sunsetr")
            .args(["S", "--json", "--follow"])
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
        {
            Ok(c) => c,
            Err(e) => {
                if let Err(send_err) = sender.send(SunsetrIpcEvents::Error(format!(
                    "sunsetr follow spawn failed: {e}"
                ))) {
                    warn!("[sunsetr/ipc] failed to send spawn error: {send_err}");
                }
                return;
            }
        };

        let stdout = match child.stdout.take() {
            Some(s) => s,
            None => return,
        };

        let mut lines = tokio::io::BufReader::new(stdout).lines();

        while let Ok(Some(line)) = lines.next_line().await {
            let line = line.trim();
            if line.is_empty() {
                continue;
            }

            if is_no_process_error(line) || line.to_lowercase().starts_with("[error]") {
                if sender
                    .send(SunsetrIpcEvents::Status(Status::inactive()))
                    .is_err()
                {
                    break;
                }
                continue;
            }

            let r = match serde_json::from_str::<SunsetrJsonEvent>(line) {
                Ok(ev) => sender.send(SunsetrIpcEvents::Status(Status::from(ev))),
                Err(_) => Ok(()),
            };
            if r.is_err() {
                break;
            }
        }
        if let Err(e) = sender.send(SunsetrIpcEvents::Status(Status::inactive())) {
            warn!("[sunsetr/ipc] failed to send inactive status on exit: {e}");
        }
        warn!("[sunsetr/ipc] follow task exited");
    });
    Ok(())
}

/* ===========================
 * CLI IPC (mirrors legacy)
 * =========================== */

async fn ipc_start() -> anyhow::Result<()> {
    run_ipc_variants(&[&["-b"], &["start"]]).await
}

async fn ipc_stop() -> anyhow::Result<()> {
    run_ipc_variants(&[&["stop"], &["off"]]).await
}

async fn ipc_status() -> anyhow::Result<Status> {
    match ipc_status_raw().await? {
        Some(ev) => Ok(Status::from(ev)),
        None => Ok(Status::inactive()),
    }
}

/// Returns:
/// - `Ok(Some(event))` when sunsetr is running and returned valid JSON
/// - `Ok(None)` when sunsetr is NOT running (the normal "no process running" case)
/// - `Err(_)` for other failures (missing binary, unexpected output, etc)
async fn ipc_status_raw() -> anyhow::Result<Option<SunsetrJsonEvent>> {
    let (code, stdout, stderr) = run_sunsetr(&["S", "--json"]).await?;

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

    let ev: SunsetrJsonEvent = serde_json::from_str(json)
        .map_err(|e| anyhow::anyhow!("Failed to parse sunsetr JSON status: {e}"))?;
    Ok(Some(ev))
}

async fn run_sunsetr(args: &[&str]) -> anyhow::Result<(i32, String, String)> {
    let out = tokio::process::Command::new("sunsetr")
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .await
        .map_err(|e| anyhow::anyhow!("Failed to run sunsetr {args:?}: {e}"))?;

    let code = out.status.code().unwrap_or(-1);
    let stdout = String::from_utf8_lossy(&out.stdout).to_string();
    let stderr = String::from_utf8_lossy(&out.stderr).to_string();
    Ok((code, stdout, stderr))
}

async fn run_ipc_variants(variants: &[&[&str]]) -> anyhow::Result<()> {
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

/// Query available sunsetr presets
pub async fn query_presets() -> anyhow::Result<Vec<String>> {
    let (code, stdout, stderr) = run_sunsetr(&["preset", "list"]).await?;

    if code != 0 {
        anyhow::bail!("sunsetr preset list failed (code {code}): {stderr}");
    }

    // Parse preset list - one preset per line
    let presets: Vec<String> = stdout
        .lines()
        .map(|line| line.trim())
        .filter(|line| !line.is_empty())
        .map(|line| line.to_string())
        .collect();

    Ok(presets)
}

/// Switch to a specific preset
pub async fn set_preset(preset_name: &str) -> anyhow::Result<()> {
    let (code, _stdout, stderr) = run_sunsetr(&["preset", preset_name]).await?;

    if code != 0 {
        anyhow::bail!("sunsetr preset {preset_name} failed (code {code}): {stderr}");
    }

    Ok(())
}

async fn refresh_after_start() -> anyhow::Result<Status> {
    // `sunsetr -b` returns before daemon is ready; wait until status JSON is available.
    let mut last_err: Option<anyhow::Error> = None;

    for attempt in 0..20 {
        match ipc_status().await {
            Ok(status) => {
                if status.active {
                    return Ok(status);
                }
            }
            Err(e) => {
                if is_no_process_error(&e.to_string()) {
                    // not-ready, retry silently
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

fn is_no_process_error(s: &str) -> bool {
    s.to_lowercase().contains(ERR_NO_PROCESS)
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

/* ===========================
 * JSON parsing + normalization
 * =========================== */

#[derive(Debug, Clone, serde::Deserialize)]
struct SunsetrJsonEvent {
    // Snapshot fields (from `sunsetr S --json`)
    period: Option<String>,
    next_period: Option<String>,

    // Some follow-mode events use `to_period`.
    to_period: Option<String>,
}

impl From<SunsetrJsonEvent> for Status {
    fn from(ev: SunsetrJsonEvent) -> Self {
        let period = ev
            .period
            .as_deref()
            .or_else(|| ev.to_period.as_deref())
            .map(|s| s.to_string());

        let next_transition_text = ev.next_period.as_deref().and_then(hhmm_from_rfc3339);

        Status {
            // If we got a JSON event, sunsetr is running (active=true)
            active: true,
            period,
            next_transition_text,
        }
    }
}
