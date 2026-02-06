//! PulseAudio/PipeWire D-Bus helpers.
//!
//! Interacts with PulseAudio or PipeWire-Pulse on the session bus.
//!
//! Note: PulseAudio's native DBus module may not be enabled by default.
//! This implementation uses the `pactl` command as a reliable fallback
//! that works with both PulseAudio and PipeWire.

use anyhow::{Context, Result, anyhow};
use std::collections::HashMap;
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

/// Card port information parsed from `pactl list cards`.
#[derive(Debug, Clone)]
pub struct CardPortInfo {
    pub product_name: Option<String>,
}

/// Key for looking up card port info: (card_id, port_name).
pub type CardPortMap = HashMap<(String, String), CardPortInfo>;

/// Sink (output device) information.
#[derive(Debug, Clone)]
#[allow(dead_code)] // volume_percent and muted are parsed but not yet consumed
pub struct SinkInfo {
    pub name: String,
    pub description: String,
    pub volume_percent: f64,
    pub muted: bool,
    pub is_default: bool,
    pub icon_name: Option<String>,
    pub bus: Option<String>,
    pub node_nick: Option<String>,
    pub device_id: Option<String>,
    pub active_port: Option<String>,
    pub active_port_available: Option<bool>,
}

/// Source (input device) information.
#[derive(Debug, Clone)]
#[allow(dead_code)] // volume_percent and muted are parsed but not yet consumed
pub struct SourceInfo {
    pub name: String,
    pub description: String,
    pub volume_percent: f64,
    pub muted: bool,
    pub is_default: bool,
    pub icon_name: Option<String>,
    pub bus: Option<String>,
    pub node_nick: Option<String>,
    pub device_id: Option<String>,
    pub active_port: Option<String>,
    pub active_port_available: Option<bool>,
}

/// Get card port info from `pactl list cards`.
pub async fn get_card_port_info() -> Result<CardPortMap> {
    let output = run_pactl(&["list", "cards"]).await?;

    if !output.status.success() {
        return Err(anyhow!(
            "pactl list cards failed: {}",
            String::from_utf8_lossy(&output.stderr)
        ));
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    Ok(parse_card_ports(&stdout))
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

    let mut sinks = parse_sinks(&stdout, default_sink.as_deref())?;

    // Filter out devices where the active port is unavailable (but keep the default)
    sinks.retain(|s| s.is_default || s.active_port_available != Some(false));

    Ok(sinks)
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

    let mut sources = parse_sources(&stdout, default_source.as_deref())?;

    // Filter out devices where the active port is unavailable (but keep the default)
    sources.retain(|s| s.is_default || s.active_port_available != Some(false));

    Ok(sources)
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

    let output = run_pactl(&[
        "set-sink-volume",
        sink_name,
        &format!("{}%", volume_percent),
    ])
    .await?;

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

    let output = run_pactl(&[
        "set-source-volume",
        source_name,
        &format!("{}%", volume_percent),
    ])
    .await?;

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
            if let Some(event) = parse_event_line(&line)
                && tx.send_async(event).await.is_err() {
                    break;
                }
        }
        let _ = child.kill().await;
    });

    Ok(handle)
}

/// Audio event types from pactl subscribe.
#[derive(Debug, Clone, PartialEq)]
pub enum AudioEvent {
    Sink,
    Source,
    Server,
    Card,
}

fn parse_event_line(line: &str) -> Option<AudioEvent> {
    let lower = line.to_lowercase();
    if lower.contains("sink") && !lower.contains("sink-input") {
        Some(AudioEvent::Sink)
    } else if lower.contains("source") && !lower.contains("source-output") {
        Some(AudioEvent::Source)
    } else if lower.contains("server") {
        Some(AudioEvent::Server)
    } else if lower.contains("card") {
        Some(AudioEvent::Card)
    } else {
        None
    }
}

fn parse_volume_percent(output: &str) -> Option<f64> {
    // pactl output looks like: "Volume: front-left: 65536 / 100% / 0.00 dB, ..."
    // We want to extract the first percentage
    for word in output.split_whitespace() {
        if let Some(pct) = word.strip_suffix('%')
            && let Ok(val) = pct.parse::<f64>() {
                return Some(val / 100.0);
            }
    }
    None
}

/// Parse a property line like `key = "value"` into (key, value).
fn parse_property_line(line: &str) -> Option<(&str, &str)> {
    let trimmed = line.trim();
    let (key, rest) = trimmed.split_once('=')?;
    let key = key.trim();
    let value = rest.trim().trim_matches('"');
    Some((key, value))
}

/// Parse the Ports section of a sink/source to determine active port availability.
///
/// The Ports section looks like:
/// ```text
/// Ports:
///     [Out] HDMI1: HDMI / DisplayPort (type: HDMI, priority: 5900, availability group: ..., not available)
///     [Out] Speaker: Speaker (type: Speaker, priority: 100, availability group: ..., available)
/// ```
fn parse_port_availability(ports_lines: &[&str], active_port: Option<&str>) -> Option<bool> {
    let active = active_port?;
    for line in ports_lines {
        let trimmed = line.trim();
        // Port lines start with the port name like "[Out] HDMI1:" or "analog-output-speaker:"
        if let Some(colon_pos) = trimmed.find(':') {
            let port_name = trimmed[..colon_pos].trim();
            if port_name == active {
                // Check for availability markers at the end
                if trimmed.contains("not available") {
                    return Some(false);
                } else if trimmed.contains("available") {
                    return Some(true);
                }
            }
        }
    }
    None
}

/// Tracking which section of pactl output we're in.
#[derive(PartialEq)]
enum ParseSection {
    Top,
    Properties,
    Ports,
}

fn parse_sinks(output: &str, default_sink: Option<&str>) -> Result<Vec<SinkInfo>> {
    let mut sinks = Vec::new();
    let mut current_name: Option<String> = None;
    let mut current_desc: Option<String> = None;
    let mut current_volume: f64 = 0.0;
    let mut current_muted: bool = false;
    let mut current_icon_name: Option<String> = None;
    let mut current_bus: Option<String> = None;
    let mut current_node_nick: Option<String> = None;
    let mut current_device_id: Option<String> = None;
    let mut current_active_port: Option<String> = None;
    let mut current_ports_lines: Vec<String> = Vec::new();
    let mut section = ParseSection::Top;

    let push_sink = |name: String,
                     desc: String,
                     volume: f64,
                     muted: bool,
                     icon_name: Option<String>,
                     bus: Option<String>,
                     node_nick: Option<String>,
                     device_id: Option<String>,
                     active_port: Option<String>,
                     ports_lines: &[String],
                     sinks: &mut Vec<SinkInfo>| {
        let port_refs: Vec<&str> = ports_lines.iter().map(|s| s.as_str()).collect();
        let active_port_available = parse_port_availability(&port_refs, active_port.as_deref());
        let is_default = default_sink.is_some_and(|d| d == name);
        sinks.push(SinkInfo {
            name,
            description: desc,
            volume_percent: volume,
            muted,
            is_default,
            icon_name,
            bus,
            node_nick,
            device_id,
            active_port,
            active_port_available,
        });
    };

    for line in output.lines() {
        let trimmed = line.trim();

        if trimmed.starts_with("Sink #") {
            // Save previous sink if exists
            if let (Some(name), Some(desc)) = (current_name.take(), current_desc.take()) {
                push_sink(
                    name,
                    desc,
                    current_volume,
                    current_muted,
                    current_icon_name.take(),
                    current_bus.take(),
                    current_node_nick.take(),
                    current_device_id.take(),
                    current_active_port.take(),
                    &current_ports_lines,
                    &mut sinks,
                );
            }
            current_volume = 0.0;
            current_muted = false;
            current_icon_name = None;
            current_bus = None;
            current_node_nick = None;
            current_device_id = None;
            current_active_port = None;
            current_ports_lines.clear();
            section = ParseSection::Top;
        } else if trimmed == "Properties:" {
            section = ParseSection::Properties;
        } else if trimmed == "Ports:" || trimmed.starts_with("Ports:") {
            section = ParseSection::Ports;
        } else if trimmed.starts_with("Active Port:") {
            current_active_port = Some(
                trimmed
                    .trim_start_matches("Active Port:")
                    .trim()
                    .to_string(),
            );
            section = ParseSection::Top;
        } else if section == ParseSection::Properties {
            // Check if we left the properties section (non-indented line that isn't a property)
            if !line.starts_with('\t') && !line.starts_with("    ") {
                section = ParseSection::Top;
            } else if let Some((key, value)) = parse_property_line(trimmed) {
                match key {
                    "device.icon_name" => current_icon_name = Some(value.to_string()),
                    "device.bus" => current_bus = Some(value.to_string()),
                    "node.nick" => current_node_nick = Some(value.to_string()),
                    "device.id" => current_device_id = Some(value.to_string()),
                    _ => {}
                }
            }
        } else if section == ParseSection::Ports {
            if !line.starts_with('\t') && !line.starts_with("    ") {
                section = ParseSection::Top;
            } else {
                current_ports_lines.push(trimmed.to_string());
            }
        }

        // These can appear at any section level
        if section == ParseSection::Top {
            if trimmed.starts_with("Name:") {
                current_name = Some(trimmed.trim_start_matches("Name:").trim().to_string());
            } else if trimmed.starts_with("Description:") {
                current_desc = Some(
                    trimmed
                        .trim_start_matches("Description:")
                        .trim()
                        .to_string(),
                );
            } else if trimmed.starts_with("Mute:") {
                current_muted = trimmed.to_lowercase().contains("yes");
            } else if trimmed.starts_with("Volume:") {
                current_volume = parse_volume_percent(trimmed).unwrap_or(0.0);
            }
        }
    }

    // Don't forget the last sink
    if let (Some(name), Some(desc)) = (current_name, current_desc) {
        push_sink(
            name,
            desc,
            current_volume,
            current_muted,
            current_icon_name,
            current_bus,
            current_node_nick,
            current_device_id,
            current_active_port,
            &current_ports_lines,
            &mut sinks,
        );
    }

    Ok(sinks)
}

fn parse_sources(output: &str, default_source: Option<&str>) -> Result<Vec<SourceInfo>> {
    let mut sources = Vec::new();
    let mut current_name: Option<String> = None;
    let mut current_desc: Option<String> = None;
    let mut current_volume: f64 = 0.0;
    let mut current_muted: bool = false;
    let mut current_icon_name: Option<String> = None;
    let mut current_bus: Option<String> = None;
    let mut current_node_nick: Option<String> = None;
    let mut current_device_id: Option<String> = None;
    let mut current_active_port: Option<String> = None;
    let mut current_ports_lines: Vec<String> = Vec::new();
    let mut section = ParseSection::Top;

    let push_source = |name: String,
                       desc: String,
                       volume: f64,
                       muted: bool,
                       icon_name: Option<String>,
                       bus: Option<String>,
                       node_nick: Option<String>,
                       device_id: Option<String>,
                       active_port: Option<String>,
                       ports_lines: &[String],
                       sources: &mut Vec<SourceInfo>| {
        if name.contains(".monitor") {
            return;
        }
        let port_refs: Vec<&str> = ports_lines.iter().map(|s| s.as_str()).collect();
        let active_port_available = parse_port_availability(&port_refs, active_port.as_deref());
        let is_default = default_source.is_some_and(|d| d == name);
        sources.push(SourceInfo {
            name,
            description: desc,
            volume_percent: volume,
            muted,
            is_default,
            icon_name,
            bus,
            node_nick,
            device_id,
            active_port,
            active_port_available,
        });
    };

    for line in output.lines() {
        let trimmed = line.trim();

        if trimmed.starts_with("Source #") {
            // Save previous source if exists
            if let (Some(name), Some(desc)) = (current_name.take(), current_desc.take()) {
                push_source(
                    name,
                    desc,
                    current_volume,
                    current_muted,
                    current_icon_name.take(),
                    current_bus.take(),
                    current_node_nick.take(),
                    current_device_id.take(),
                    current_active_port.take(),
                    &current_ports_lines,
                    &mut sources,
                );
            }
            current_volume = 0.0;
            current_muted = false;
            current_icon_name = None;
            current_bus = None;
            current_node_nick = None;
            current_device_id = None;
            current_active_port = None;
            current_ports_lines.clear();
            section = ParseSection::Top;
        } else if trimmed == "Properties:" {
            section = ParseSection::Properties;
        } else if trimmed == "Ports:" || trimmed.starts_with("Ports:") {
            section = ParseSection::Ports;
        } else if trimmed.starts_with("Active Port:") {
            current_active_port = Some(
                trimmed
                    .trim_start_matches("Active Port:")
                    .trim()
                    .to_string(),
            );
            section = ParseSection::Top;
        } else if section == ParseSection::Properties {
            if !line.starts_with('\t') && !line.starts_with("    ") {
                section = ParseSection::Top;
            } else if let Some((key, value)) = parse_property_line(trimmed) {
                match key {
                    "device.icon_name" => current_icon_name = Some(value.to_string()),
                    "device.bus" => current_bus = Some(value.to_string()),
                    "node.nick" => current_node_nick = Some(value.to_string()),
                    "device.id" => current_device_id = Some(value.to_string()),
                    _ => {}
                }
            }
        } else if section == ParseSection::Ports {
            if !line.starts_with('\t') && !line.starts_with("    ") {
                section = ParseSection::Top;
            } else {
                current_ports_lines.push(trimmed.to_string());
            }
        }

        if section == ParseSection::Top {
            if trimmed.starts_with("Name:") {
                current_name = Some(trimmed.trim_start_matches("Name:").trim().to_string());
            } else if trimmed.starts_with("Description:") {
                current_desc = Some(
                    trimmed
                        .trim_start_matches("Description:")
                        .trim()
                        .to_string(),
                );
            } else if trimmed.starts_with("Mute:") {
                current_muted = trimmed.to_lowercase().contains("yes");
            } else if trimmed.starts_with("Volume:") {
                current_volume = parse_volume_percent(trimmed).unwrap_or(0.0);
            }
        }
    }

    // Don't forget the last source
    if let (Some(name), Some(desc)) = (current_name, current_desc) {
        push_source(
            name,
            desc,
            current_volume,
            current_muted,
            current_icon_name,
            current_bus,
            current_node_nick,
            current_device_id,
            current_active_port,
            &current_ports_lines,
            &mut sources,
        );
    }

    Ok(sources)
}

/// Parse `pactl list cards` output to extract port-level product names.
fn parse_card_ports(output: &str) -> CardPortMap {
    let mut map = CardPortMap::new();
    let mut current_card_id: Option<String> = None;
    let mut in_ports = false;
    let mut current_port_name: Option<String> = None;
    let mut in_port_properties = false;
    let mut current_product_name: Option<String> = None;

    for line in output.lines() {
        let trimmed = line.trim();

        // Card #49
        if trimmed.starts_with("Card #") {
            // Save previous port info if any
            if let (Some(card_id), Some(port_name)) =
                (current_card_id.as_ref(), current_port_name.take())
            {
                map.insert(
                    (card_id.clone(), port_name),
                    CardPortInfo {
                        product_name: current_product_name.take(),
                    },
                );
            }
            current_card_id = trimmed.strip_prefix("Card #").map(|s| s.to_string());
            in_ports = false;
            in_port_properties = false;
            current_port_name = None;
            current_product_name = None;
        } else if trimmed == "Ports:" {
            in_ports = true;
            in_port_properties = false;
        } else if in_ports {
            // Detect end of Ports section: a top-level line that isn't indented enough
            // Port entries are indented with tabs; sub-properties are further indented
            let indent_level = line.len() - line.trim_start().len();

            if indent_level == 0 || (!line.starts_with('\t') && !line.starts_with("    ")) {
                // Save previous port info if any
                if let (Some(card_id), Some(port_name)) =
                    (current_card_id.as_ref(), current_port_name.take())
                {
                    map.insert(
                        (card_id.clone(), port_name),
                        CardPortInfo {
                            product_name: current_product_name.take(),
                        },
                    );
                }
                in_ports = false;
                in_port_properties = false;
                continue;
            }

            if trimmed == "Properties:" {
                in_port_properties = true;
            } else if in_port_properties {
                // Check if we've left the properties section
                // Properties are more deeply indented than the "Properties:" header
                if let Some((key, value)) = parse_property_line(trimmed) {
                    if key == "device.product.name" {
                        current_product_name = Some(value.to_string());
                    }
                } else {
                    // Non-property line, likely a new port or section
                    in_port_properties = false;
                }
            }

            if !in_port_properties
                && trimmed.contains(':')
                && !trimmed.starts_with("Part of profile")
            {
                // This might be a new port line like:
                // [Out] HDMI1: HDMI / DisplayPort (type: HDMI, priority: 5900, ...)
                // Save previous port
                if let (Some(card_id), Some(port_name)) =
                    (current_card_id.as_ref(), current_port_name.take())
                {
                    map.insert(
                        (card_id.clone(), port_name),
                        CardPortInfo {
                            product_name: current_product_name.take(),
                        },
                    );
                }
                current_product_name = None;

                // Extract port name (everything before the first colon)
                if let Some(colon_pos) = trimmed.find(':') {
                    let name = trimmed[..colon_pos].trim();
                    if !name.is_empty() && name != "Properties" {
                        current_port_name = Some(name.to_string());
                    }
                }
            }
        }
    }

    // Save final port info
    if let (Some(card_id), Some(port_name)) = (current_card_id, current_port_name) {
        map.insert(
            (card_id, port_name),
            CardPortInfo {
                product_name: current_product_name,
            },
        );
    }

    map
}

/// Compute the primary icon for a sink.
fn compute_primary_icon_sink(icon_name: &Option<String>) -> String {
    match icon_name {
        Some(name) if !name.is_empty() => {
            if name.ends_with("-symbolic") {
                name.clone()
            } else {
                format!("{}-symbolic", name)
            }
        }
        _ => "audio-speakers-symbolic".to_string(),
    }
}

/// Compute the primary icon for a source.
fn compute_primary_icon_source(icon_name: &Option<String>) -> String {
    match icon_name {
        Some(name) if !name.is_empty() => {
            if name.ends_with("-symbolic") {
                name.clone()
            } else {
                format!("{}-symbolic", name)
            }
        }
        _ => "audio-input-microphone-symbolic".to_string(),
    }
}

/// Compute the secondary icon based on device properties.
fn compute_secondary_icon(icon_name: &Option<String>, bus: &Option<String>) -> Option<String> {
    if icon_name.as_deref() == Some("video-display") {
        Some("video-joined-displays-symbolic".to_string())
    } else if bus.as_deref() == Some("bluetooth") {
        Some("bluetooth-symbolic".to_string())
    } else {
        None
    }
}

/// Compute the display label for a device.
fn compute_label(
    description: &str,
    node_nick: &Option<String>,
    device_id: &Option<String>,
    active_port: &Option<String>,
    icon_name: &Option<String>,
    bus: &Option<String>,
    card_ports: &CardPortMap,
) -> String {
    // 1. Try card port product name (EDID data for connected displays)
    if let (Some(dev_id), Some(port)) = (device_id, active_port) {
        let key = (dev_id.clone(), port.clone());
        if let Some(port_info) = card_ports.get(&key)
            && let Some(ref product) = port_info.product_name
                && !product.is_empty() {
                    return product.clone();
                }
    }

    // 2. Use node.nick as base, fall back to description
    let base = node_nick
        .as_deref()
        .filter(|s| !s.is_empty())
        .unwrap_or(description);

    let mut label = base.to_string();

    // 3. Strip trailing " Output" / " Input"
    if let Some(stripped) = label.strip_suffix(" Output") {
        label = stripped.to_string();
    } else if let Some(stripped) = label.strip_suffix(" Input") {
        label = stripped.to_string();
    }

    let has_hdmi_secondary = icon_name.as_deref() == Some("video-display");
    let has_bt_secondary = bus.as_deref() == Some("bluetooth");

    // 4. Strip "HDMI / " prefix for HDMI devices (but keep "DisplayPort N")
    if has_hdmi_secondary
        && let Some(stripped) = label.strip_prefix("HDMI / ") {
            label = stripped.to_string();
        }

    // 5. Strip "Bluetooth " prefix for Bluetooth devices
    if has_bt_secondary
        && let Some(stripped) = label.strip_prefix("Bluetooth ") {
            label = stripped.to_string();
        }

    // 6. Fall back to description if result is empty
    if label.is_empty() {
        label = description.to_string();
    }

    label
}

impl AudioDevice {
    /// Create an AudioDevice from a SinkInfo with card port context.
    pub fn from_sink(sink: &SinkInfo, card_ports: &CardPortMap) -> Self {
        let icon = compute_primary_icon_sink(&sink.icon_name);
        let secondary_icon = compute_secondary_icon(&sink.icon_name, &sink.bus);
        let name = compute_label(
            &sink.description,
            &sink.node_nick,
            &sink.device_id,
            &sink.active_port,
            &sink.icon_name,
            &sink.bus,
            card_ports,
        );

        AudioDevice {
            id: sink.name.clone(),
            name,
            icon,
            secondary_icon,
        }
    }

    /// Create an AudioDevice from a SourceInfo with card port context.
    pub fn from_source(source: &SourceInfo, card_ports: &CardPortMap) -> Self {
        let icon = compute_primary_icon_source(&source.icon_name);
        let secondary_icon = compute_secondary_icon(&source.icon_name, &source.bus);
        let name = compute_label(
            &source.description,
            &source.node_nick,
            &source.device_id,
            &source.active_port,
            &source.icon_name,
            &source.bus,
            card_ports,
        );

        AudioDevice {
            id: source.name.clone(),
            name,
            icon,
            secondary_icon,
        }
    }
}

#[cfg(test)]
#[path = "dbus_tests.rs"]
mod tests;
