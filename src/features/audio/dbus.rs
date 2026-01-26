//! PulseAudio/PipeWire D-Bus helpers.
//!
//! Interacts with PulseAudio or PipeWire-Pulse on the session bus.
//!
//! Note: PulseAudio's native DBus module may not be enabled by default.
//! This implementation uses the `pactl` command as a reliable fallback
//! that works with both PulseAudio and PipeWire.

use anyhow::{anyhow, Context, Result};
use std::process::Stdio;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;

use super::store::AudioDevice;
use crate::runtime::spawn_on_tokio;

/// Run a one-shot `pactl` command on the tokio runtime.
///
/// This ensures child processes are properly reaped and avoids busy-polling
/// when called from a glib async context.
async fn run_pactl(args: &[&str]) -> Result<std::process::Output> {
    let args: Vec<String> = args.iter().map(|s| s.to_string()).collect();
    spawn_on_tokio(async move {
        Command::new("pactl")
            .args(&args)
            .output()
            .await
            .context("Failed to execute pactl")
    })
    .await
}

/// Sink (output device) information.
#[derive(Debug, Clone)]
pub struct SinkInfo {
    pub name: String,
    pub description: String,
    pub volume_percent: f64,
    pub muted: bool,
    pub is_default: bool,
}

/// Source (input device) information.
#[derive(Debug, Clone)]
pub struct SourceInfo {
    pub name: String,
    pub description: String,
    pub volume_percent: f64,
    pub muted: bool,
    pub is_default: bool,
}

/// Get all available sinks (output devices).
pub async fn get_sinks() -> Result<Vec<SinkInfo>> {
    let output = run_pactl(&["list", "sinks"]).await?;

    if !output.status.success() {
        return Err(anyhow!(
            "pactl list sinks failed: {}",
            String::from_utf8_lossy(&output.stderr)
        ));
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let default_sink = get_default_sink().await.ok();

    parse_sinks(&stdout, default_sink.as_deref())
}

/// Get all available sources (input devices).
pub async fn get_sources() -> Result<Vec<SourceInfo>> {
    let output = run_pactl(&["list", "sources"]).await?;

    if !output.status.success() {
        return Err(anyhow!(
            "pactl list sources failed: {}",
            String::from_utf8_lossy(&output.stderr)
        ));
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let default_source = get_default_source().await.ok();

    parse_sources(&stdout, default_source.as_deref())
}

/// Get the default sink name.
pub async fn get_default_sink() -> Result<String> {
    let output = run_pactl(&["get-default-sink"]).await?;

    if !output.status.success() {
        return Err(anyhow!(
            "pactl get-default-sink failed: {}",
            String::from_utf8_lossy(&output.stderr)
        ));
    }

    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

/// Get the default source name.
pub async fn get_default_source() -> Result<String> {
    let output = run_pactl(&["get-default-source"]).await?;

    if !output.status.success() {
        return Err(anyhow!(
            "pactl get-default-source failed: {}",
            String::from_utf8_lossy(&output.stderr)
        ));
    }

    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

/// Get the current sink volume and mute state.
pub async fn get_sink_volume(sink_name: &str) -> Result<(f64, bool)> {
    let output = run_pactl(&["get-sink-volume", sink_name]).await?;

    if !output.status.success() {
        return Err(anyhow!(
            "pactl get-sink-volume failed: {}",
            String::from_utf8_lossy(&output.stderr)
        ));
    }

    let volume_str = String::from_utf8_lossy(&output.stdout);
    let volume = parse_volume_percent(&volume_str).unwrap_or(0.0);

    let mute_output = run_pactl(&["get-sink-mute", sink_name]).await?;

    let mute_str = String::from_utf8_lossy(&mute_output.stdout);
    let muted = mute_str.to_lowercase().contains("yes");

    Ok((volume, muted))
}

/// Get the current source volume and mute state.
pub async fn get_source_volume(source_name: &str) -> Result<(f64, bool)> {
    let output = run_pactl(&["get-source-volume", source_name]).await?;

    if !output.status.success() {
        return Err(anyhow!(
            "pactl get-source-volume failed: {}",
            String::from_utf8_lossy(&output.stderr)
        ));
    }

    let volume_str = String::from_utf8_lossy(&output.stdout);
    let volume = parse_volume_percent(&volume_str).unwrap_or(0.0);

    let mute_output = run_pactl(&["get-source-mute", source_name]).await?;

    let mute_str = String::from_utf8_lossy(&mute_output.stdout);
    let muted = mute_str.to_lowercase().contains("yes");

    Ok((volume, muted))
}

/// Set the sink (output) volume.
pub async fn set_sink_volume(sink_name: &str, volume: f64) -> Result<()> {
    let volume_percent = (volume.clamp(0.0, 1.0) * 100.0).round() as u32;

    let output = run_pactl(&["set-sink-volume", sink_name, &format!("{}%", volume_percent)]).await?;

    if !output.status.success() {
        return Err(anyhow!(
            "pactl set-sink-volume failed: {}",
            String::from_utf8_lossy(&output.stderr)
        ));
    }

    Ok(())
}

/// Set the source (input) volume.
pub async fn set_source_volume(source_name: &str, volume: f64) -> Result<()> {
    let volume_percent = (volume.clamp(0.0, 1.0) * 100.0).round() as u32;

    let output = run_pactl(&["set-source-volume", source_name, &format!("{}%", volume_percent)]).await?;

    if !output.status.success() {
        return Err(anyhow!(
            "pactl set-source-volume failed: {}",
            String::from_utf8_lossy(&output.stderr)
        ));
    }

    Ok(())
}

/// Set the sink (output) mute state.
pub async fn set_sink_mute(sink_name: &str, muted: bool) -> Result<()> {
    let mute_arg = if muted { "1" } else { "0" };

    let output = run_pactl(&["set-sink-mute", sink_name, mute_arg]).await?;

    if !output.status.success() {
        return Err(anyhow!(
            "pactl set-sink-mute failed: {}",
            String::from_utf8_lossy(&output.stderr)
        ));
    }

    Ok(())
}

/// Set the source (input) mute state.
pub async fn set_source_mute(source_name: &str, muted: bool) -> Result<()> {
    let mute_arg = if muted { "1" } else { "0" };

    let output = run_pactl(&["set-source-mute", source_name, mute_arg]).await?;

    if !output.status.success() {
        return Err(anyhow!(
            "pactl set-source-mute failed: {}",
            String::from_utf8_lossy(&output.stderr)
        ));
    }

    Ok(())
}

/// Set the default sink (output device).
pub async fn set_default_sink(sink_name: &str) -> Result<()> {
    let output = run_pactl(&["set-default-sink", sink_name]).await?;

    if !output.status.success() {
        return Err(anyhow!(
            "pactl set-default-sink failed: {}",
            String::from_utf8_lossy(&output.stderr)
        ));
    }

    Ok(())
}

/// Set the default source (input device).
pub async fn set_default_source(source_name: &str) -> Result<()> {
    let output = run_pactl(&["set-default-source", source_name]).await?;

    if !output.status.success() {
        return Err(anyhow!(
            "pactl set-default-source failed: {}",
            String::from_utf8_lossy(&output.stderr)
        ));
    }

    Ok(())
}

/// Check if PulseAudio/PipeWire is available.
pub async fn is_available() -> bool {
    run_pactl(&["info"])
        .await
        .map(|o| o.status.success())
        .unwrap_or(false)
}

/// Subscribe to PulseAudio events.
/// Returns a channel that receives event types.
pub async fn subscribe_events(
    tx: flume::Sender<AudioEvent>,
) -> Result<tokio::task::JoinHandle<()>> {
    let mut child = Command::new("pactl")
        .args(["subscribe"])
        .stdout(Stdio::piped())
        .spawn()
        .context("Failed to spawn pactl subscribe")?;

    let stdout = child.stdout.take().ok_or_else(|| anyhow!("No stdout"))?;
    let reader = BufReader::new(stdout);
    let mut lines = reader.lines();

    let handle = tokio::spawn(async move {
        while let Ok(Some(line)) = lines.next_line().await {
            if let Some(event) = parse_event_line(&line) {
                if tx.send_async(event).await.is_err() {
                    break;
                }
            }
        }
        let _ = child.kill().await;
    });

    Ok(handle)
}

/// Audio event types from pactl subscribe.
#[derive(Debug, Clone)]
pub enum AudioEvent {
    SinkChange,
    SourceChange,
    ServerChange,
    CardChange,
}

fn parse_event_line(line: &str) -> Option<AudioEvent> {
    let lower = line.to_lowercase();
    if lower.contains("sink") && !lower.contains("sink-input") {
        Some(AudioEvent::SinkChange)
    } else if lower.contains("source") && !lower.contains("source-output") {
        Some(AudioEvent::SourceChange)
    } else if lower.contains("server") {
        Some(AudioEvent::ServerChange)
    } else if lower.contains("card") {
        Some(AudioEvent::CardChange)
    } else {
        None
    }
}

fn parse_volume_percent(output: &str) -> Option<f64> {
    // pactl output looks like: "Volume: front-left: 65536 / 100% / 0.00 dB, ..."
    // We want to extract the first percentage
    for word in output.split_whitespace() {
        if let Some(pct) = word.strip_suffix('%') {
            if let Ok(val) = pct.parse::<f64>() {
                return Some(val / 100.0);
            }
        }
    }
    None
}

fn parse_sinks(output: &str, default_sink: Option<&str>) -> Result<Vec<SinkInfo>> {
    let mut sinks = Vec::new();
    let mut current_name: Option<String> = None;
    let mut current_desc: Option<String> = None;
    let mut current_volume: f64 = 0.0;
    let mut current_muted: bool = false;

    for line in output.lines() {
        let trimmed = line.trim();

        if trimmed.starts_with("Sink #") {
            // Save previous sink if exists
            if let (Some(name), Some(desc)) = (current_name.take(), current_desc.take()) {
                let is_default = default_sink.map_or(false, |d| d == name);
                sinks.push(SinkInfo {
                    name,
                    description: desc,
                    volume_percent: current_volume,
                    muted: current_muted,
                    is_default,
                });
            }
            current_volume = 0.0;
            current_muted = false;
        } else if trimmed.starts_with("Name:") {
            current_name = Some(trimmed.trim_start_matches("Name:").trim().to_string());
        } else if trimmed.starts_with("Description:") {
            current_desc = Some(trimmed.trim_start_matches("Description:").trim().to_string());
        } else if trimmed.starts_with("Mute:") {
            current_muted = trimmed.to_lowercase().contains("yes");
        } else if trimmed.starts_with("Volume:") {
            current_volume = parse_volume_percent(trimmed).unwrap_or(0.0);
        }
    }

    // Don't forget the last sink
    if let (Some(name), Some(desc)) = (current_name, current_desc) {
        let is_default = default_sink.map_or(false, |d| d == name);
        sinks.push(SinkInfo {
            name,
            description: desc,
            volume_percent: current_volume,
            muted: current_muted,
            is_default,
        });
    }

    Ok(sinks)
}

fn parse_sources(output: &str, default_source: Option<&str>) -> Result<Vec<SourceInfo>> {
    let mut sources = Vec::new();
    let mut current_name: Option<String> = None;
    let mut current_desc: Option<String> = None;
    let mut current_volume: f64 = 0.0;
    let mut current_muted: bool = false;

    for line in output.lines() {
        let trimmed = line.trim();

        if trimmed.starts_with("Source #") {
            // Save previous source if exists
            if let (Some(name), Some(desc)) = (current_name.take(), current_desc.take()) {
                // Filter out monitor sources (they mirror output)
                if !name.contains(".monitor") {
                    let is_default = default_source.map_or(false, |d| d == name);
                    sources.push(SourceInfo {
                        name,
                        description: desc,
                        volume_percent: current_volume,
                        muted: current_muted,
                        is_default,
                    });
                }
            }
            current_volume = 0.0;
            current_muted = false;
        } else if trimmed.starts_with("Name:") {
            current_name = Some(trimmed.trim_start_matches("Name:").trim().to_string());
        } else if trimmed.starts_with("Description:") {
            current_desc = Some(trimmed.trim_start_matches("Description:").trim().to_string());
        } else if trimmed.starts_with("Mute:") {
            current_muted = trimmed.to_lowercase().contains("yes");
        } else if trimmed.starts_with("Volume:") {
            current_volume = parse_volume_percent(trimmed).unwrap_or(0.0);
        }
    }

    // Don't forget the last source
    if let (Some(name), Some(desc)) = (current_name, current_desc) {
        if !name.contains(".monitor") {
            let is_default = default_source.map_or(false, |d| d == name);
            sources.push(SourceInfo {
                name,
                description: desc,
                volume_percent: current_volume,
                muted: current_muted,
                is_default,
            });
        }
    }

    Ok(sources)
}

/// Convert SinkInfo to AudioDevice.
impl From<SinkInfo> for AudioDevice {
    fn from(sink: SinkInfo) -> Self {
        AudioDevice {
            id: sink.name,
            name: sink.description,
            icon: "audio-speakers-symbolic".to_string(),
        }
    }
}

/// Convert SourceInfo to AudioDevice.
impl From<SourceInfo> for AudioDevice {
    fn from(source: SourceInfo) -> Self {
        AudioDevice {
            id: source.name,
            name: source.description,
            icon: "audio-input-microphone-symbolic".to_string(),
        }
    }
}
