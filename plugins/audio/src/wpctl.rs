use std::process::Stdio;

use anyhow::{Context, Result, anyhow};

use crate::pactl::{compute_connection_type, compute_device_type};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WpctlDeviceKind {
    Sink,
    Source,
}

#[derive(Clone, Debug, PartialEq)]
pub struct WpctlDevice {
    pub id: String,
    pub name: String,
    pub volume: f64,
    pub muted: bool,
    pub default: bool,
    pub kind: WpctlDeviceKind,
    pub device_type: String,
    pub connection_type: Option<String>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct WpctlSnapshot {
    pub sinks: Vec<WpctlDevice>,
    pub sources: Vec<WpctlDevice>,
    pub default_sink: Option<String>,
    pub default_source: Option<String>,
}

async fn run_wpctl(args: &[&str]) -> Result<std::process::Output> {
    tokio::process::Command::new("wpctl")
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .await
        .context("failed to execute wpctl")
}

pub async fn is_available() -> bool {
    match run_wpctl(&["status", "--name"]).await {
        Ok(output) => output.status.success(),
        Err(_) => false,
    }
}

pub async fn snapshot() -> Result<WpctlSnapshot> {
    let output = run_wpctl(&["status", "--name"]).await?;
    if !output.status.success() {
        return Err(anyhow!(
            "wpctl status failed: {}",
            String::from_utf8_lossy(&output.stderr)
        ));
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    parse_status(&stdout).await
}

pub async fn set_volume(id: &str, volume: f64) -> Result<()> {
    let percent = (volume.clamp(0.0, 1.0) * 100.0).round() as u32;
    let output = run_wpctl(&["set-volume", id, &format!("{percent}%")]).await?;
    if !output.status.success() {
        return Err(anyhow!(
            "wpctl set-volume failed: {}",
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    Ok(())
}

pub async fn set_mute(id: &str, muted: bool) -> Result<()> {
    let output = run_wpctl(&["set-mute", id, if muted { "1" } else { "0" }]).await?;
    if !output.status.success() {
        return Err(anyhow!(
            "wpctl set-mute failed: {}",
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    Ok(())
}

pub async fn set_default(id: &str) -> Result<()> {
    let output = run_wpctl(&["set-default", id]).await?;
    if !output.status.success() {
        return Err(anyhow!(
            "wpctl set-default failed: {}",
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    Ok(())
}

async fn parse_status(status: &str) -> Result<WpctlSnapshot> {
    let mut sinks = Vec::new();
    let mut sources = Vec::new();
    let mut section = None;

    for line in status.lines() {
        let trimmed = line.trim();
        if trimmed == "├─ Sinks:" {
            section = Some(WpctlDeviceKind::Sink);
            continue;
        }
        if trimmed == "├─ Sources:" {
            section = Some(WpctlDeviceKind::Source);
            continue;
        }
        if trimmed.starts_with("├─ ") || trimmed.starts_with("└─ ") {
            section = None;
            continue;
        }

        let Some(kind) = section else { continue };
        let Some(parsed) = parse_device_line(trimmed, kind) else {
            continue;
        };

        let enriched = enrich_device(parsed).await?;
        match enriched.kind {
            WpctlDeviceKind::Sink => sinks.push(enriched),
            WpctlDeviceKind::Source => sources.push(enriched),
        }
    }

    let default_sink = sinks.iter().find(|d| d.default).map(|d| d.id.clone());
    let default_source = sources.iter().find(|d| d.default).map(|d| d.id.clone());

    Ok(WpctlSnapshot {
        sinks,
        sources,
        default_sink,
        default_source,
    })
}

fn parse_device_line(line: &str, kind: WpctlDeviceKind) -> Option<WpctlDevice> {
    if !line.contains("[vol:") || !line.contains('.') {
        return None;
    }

    let default = line.contains('*');
    let normalized: String = line
        .chars()
        .skip_while(|c| !c.is_ascii_digit() && *c != '*')
        .collect();
    let normalized = normalized.replace('*', " ");
    let normalized = normalized.trim();
    let dot = normalized.find('.')?;
    let id = normalized[..dot].trim().to_string();
    let rest = normalized[dot + 1..].trim();
    let bracket = rest.rfind("[")?;
    let raw_name = rest[..bracket].trim().to_string();
    let info = rest[bracket..].trim();

    let vol_marker = "vol:";
    let vol_start = info.find(vol_marker)? + vol_marker.len();
    let vol_end = info[vol_start..]
        .find(']')
        .map(|idx| vol_start + idx)
        .unwrap_or(info.len());
    let volume = info[vol_start..vol_end]
        .split_whitespace()
        .next()?
        .trim()
        .parse::<f64>()
        .ok()?;
    let muted = info.contains("MUTED");

    Some(WpctlDevice {
        id,
        name: raw_name,
        volume,
        muted,
        default,
        kind,
        device_type: String::new(),
        connection_type: None,
    })
}

async fn enrich_device(mut device: WpctlDevice) -> Result<WpctlDevice> {
    let output = run_wpctl(&["inspect", &device.id]).await?;
    if !output.status.success() {
        return Ok(device);
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let node_description = extract_quoted(&stdout, "node.description = ");
    let node_nick = extract_quoted(&stdout, "node.nick = ");
    let bus = extract_quoted(&stdout, "device.bus = ");
    let icon_name = extract_quoted(&stdout, "device.icon-name = ");

    device.name = node_description.or(node_nick).unwrap_or(device.name);
    device.connection_type = compute_connection_type(bus.as_deref(), None);
    device.device_type = compute_device_type(
        None,
        icon_name.as_deref(),
        None,
        matches!(device.kind, WpctlDeviceKind::Source),
    );

    Ok(device)
}

fn extract_quoted(text: &str, needle: &str) -> Option<String> {
    text.lines().find_map(|line| {
        let idx = line.find(needle)?;
        let value = line[idx + needle.len()..].trim();
        value
            .strip_prefix('"')?
            .strip_suffix('"')
            .map(std::string::ToString::to_string)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_sink_line() {
        let device = parse_device_line(
            "│  *   59. alsa_output.pci-0000_0a_00.3.analog-stereo [vol: 0.25]",
            WpctlDeviceKind::Sink,
        )
        .expect("device");
        assert_eq!(device.id, "59");
        assert_eq!(device.name, "alsa_output.pci-0000_0a_00.3.analog-stereo");
        assert!(device.default);
        assert!(!device.muted);
        assert!((device.volume - 0.25).abs() < 0.001);
    }

    #[test]
    fn parses_source_line_with_muted() {
        let device = parse_device_line(
            "│  *   64. alsa_input.usb-mic [vol: 1.00 MUTED]",
            WpctlDeviceKind::Source,
        )
        .expect("device");
        assert_eq!(device.id, "64");
        assert!(device.default);
        assert!(device.muted);
        assert!((device.volume - 1.0).abs() < 0.001);
    }
}
