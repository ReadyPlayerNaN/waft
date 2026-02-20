//! PulseAudio/PipeWire pactl command helpers.
//!
//! Interacts with PulseAudio or PipeWire-Pulse via the `pactl` command.
//! This works reliably with both PulseAudio and PipeWire.

use anyhow::{Context, Result, anyhow};
use std::collections::HashMap;
use std::process::Stdio;

/// Represents an audio device (output or input).
#[derive(Clone, Debug, PartialEq)]
pub struct AudioDevice {
    pub id: String,
    pub name: String,
    pub icon: String,
    pub secondary_icon: Option<String>,
    pub volume: f64,
    pub muted: bool,
}

/// Card port information parsed from `pactl list cards`.
#[derive(Debug, Clone)]
pub struct CardPortInfo {
    pub product_name: Option<String>,
}

/// Key for looking up card port info: (card_id, port_name).
pub type CardPortMap = HashMap<(String, String), CardPortInfo>;

/// A port on a sink or source.
#[derive(Debug, Clone, PartialEq)]
pub struct PortInfo {
    pub name: String,
    pub description: String,
    pub available: bool,
    pub port_type: Option<String>,
}

/// Sink (output device) information.
#[derive(Debug, Clone)]
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
    pub form_factor: Option<String>,
    pub active_port: Option<String>,
    pub active_port_available: Option<bool>,
    /// Parsed ports with name, description, and availability.
    pub ports: Vec<PortInfo>,
}

/// Source (input device) information.
#[derive(Debug, Clone)]
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
    pub form_factor: Option<String>,
    pub active_port: Option<String>,
    pub active_port_available: Option<bool>,
    /// Parsed ports with name, description, and availability.
    pub ports: Vec<PortInfo>,
}

/// Card-level information parsed from `pactl list cards`.
#[derive(Debug, Clone)]
pub struct CardInfo {
    pub name: String,
    pub description: String,
    pub icon_name: Option<String>,
    pub bus: Option<String>,
    pub form_factor: Option<String>,
    pub profiles: Vec<CardProfile>,
    pub active_profile: String,
}

/// A profile on a card.
#[derive(Debug, Clone)]
pub struct CardProfile {
    pub name: String,
    pub description: String,
    pub available: bool,
}

/// Audio event types from pactl subscribe.
#[derive(Debug, Clone, PartialEq)]
pub enum AudioEvent {
    Sink,
    Source,
    Server,
    Card,
}

/// Run a one-shot `pactl` command via tokio process.
async fn run_pactl(args: &[&str]) -> Result<std::process::Output> {
    tokio::process::Command::new("pactl")
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .await
        .context("Failed to execute pactl")
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

/// Get all audio cards with profiles.
pub async fn get_cards() -> Result<Vec<CardInfo>> {
    let output = run_pactl(&["list", "cards"]).await?;

    if !output.status.success() {
        return Err(anyhow!(
            "pactl list cards failed: {}",
            String::from_utf8_lossy(&output.stderr)
        ));
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    Ok(parse_cards(&stdout))
}

/// Set the active profile on a card.
pub async fn set_card_profile(card_name: &str, profile: &str) -> Result<()> {
    let output = run_pactl(&["set-card-profile", card_name, profile]).await?;

    if !output.status.success() {
        return Err(anyhow!(
            "pactl set-card-profile failed: {}",
            String::from_utf8_lossy(&output.stderr)
        ));
    }

    Ok(())
}

/// Set the active port on a sink.
pub async fn set_sink_port(sink_name: &str, port: &str) -> Result<()> {
    let output = run_pactl(&["set-sink-port", sink_name, port]).await?;

    if !output.status.success() {
        return Err(anyhow!(
            "pactl set-sink-port failed: {}",
            String::from_utf8_lossy(&output.stderr)
        ));
    }

    Ok(())
}

/// Set the active port on a source.
pub async fn set_source_port(source_name: &str, port: &str) -> Result<()> {
    let output = run_pactl(&["set-source-port", source_name, port]).await?;

    if !output.status.success() {
        return Err(anyhow!(
            "pactl set-source-port failed: {}",
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
///
/// Spawns a background thread that reads events from `pactl subscribe`
/// and sends them via a tokio mpsc channel.
pub fn subscribe_events() -> Result<tokio::sync::mpsc::Receiver<AudioEvent>> {
    let (tx, rx) = tokio::sync::mpsc::channel(32);

    let mut child = std::process::Command::new("pactl")
        .args(["subscribe"])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .context("Failed to spawn pactl subscribe")?;

    let stdout = child.stdout.take().ok_or_else(|| anyhow!("No stdout"))?;

    std::thread::spawn(move || {
        use std::io::BufRead;
        let reader = std::io::BufReader::new(stdout);
        for line in reader.lines() {
            match line {
                Ok(line) => {
                    if let Some(event) = parse_event_line(&line)
                        && tx.blocking_send(event).is_err()
                    {
                        break;
                    }
                }
                Err(_) => break,
            }
        }
        let _ = child.kill();
    });

    Ok(rx)
}

pub fn parse_event_line(line: &str) -> Option<AudioEvent> {
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

pub fn parse_volume_percent(output: &str) -> Option<f64> {
    // pactl output looks like: "Volume: front-left: 65536 / 100% / 0.00 dB, ..."
    // We want to extract the first percentage
    for word in output.split_whitespace() {
        if let Some(pct) = word.strip_suffix('%')
            && let Ok(val) = pct.parse::<f64>()
        {
            return Some(val / 100.0);
        }
    }
    None
}

/// Parse a property line like `key = "value"` into (key, value).
pub fn parse_property_line(line: &str) -> Option<(&str, &str)> {
    let trimmed = line.trim();
    let (key, rest) = trimmed.split_once('=')?;
    let key = key.trim();
    let value = rest.trim().trim_matches('"');
    Some((key, value))
}

/// Parse a single port line into a PortInfo.
///
/// Port lines look like:
/// ```text
/// [Out] HDMI1: HDMI / DisplayPort (type: HDMI, priority: 5900, availability group: ..., not available)
/// analog-output-speaker: Speaker (type: Speaker, priority: 100, availability group: ..., available)
/// ```
fn parse_port_line(line: &str) -> Option<PortInfo> {
    let trimmed = line.trim();
    let colon_pos = trimmed.find(':')?;
    let name = trimmed[..colon_pos].trim();
    if name.is_empty() || name == "Properties" {
        return None;
    }

    // Extract description: text between first ":" and first "(" or end of line
    let after_colon = trimmed[colon_pos + 1..].trim();
    let description = if let Some(paren_pos) = after_colon.find('(') {
        after_colon[..paren_pos].trim().to_string()
    } else {
        after_colon.to_string()
    };

    // Extract port type from parenthesized metadata: "(type: Speaker, ...)"
    let port_type = if let Some(paren_pos) = after_colon.find('(') {
        let meta = &after_colon[paren_pos..];
        if let Some(type_start) = meta.find("type: ") {
            let after_type = &meta[type_start + 6..];
            let end = after_type.find(',').or_else(|| after_type.find(')'));
            end.map(|pos| after_type[..pos].trim().to_string())
        } else {
            None
        }
    } else {
        None
    };

    // Determine availability
    let available = if trimmed.contains("not available") {
        false
    } else {
        trimmed.contains("available")
    };

    Some(PortInfo {
        name: name.to_string(),
        description,
        available,
        port_type,
    })
}

/// Parse port lines into structured PortInfo list.
fn parse_ports_structured(ports_lines: &[String]) -> Vec<PortInfo> {
    ports_lines
        .iter()
        .filter_map(|line| parse_port_line(line))
        .collect()
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

pub fn parse_sinks(output: &str, default_sink: Option<&str>) -> Result<Vec<SinkInfo>> {
    let mut sinks = Vec::new();
    let mut current_name: Option<String> = None;
    let mut current_desc: Option<String> = None;
    let mut current_volume: f64 = 0.0;
    let mut current_muted: bool = false;
    let mut current_icon_name: Option<String> = None;
    let mut current_bus: Option<String> = None;
    let mut current_node_nick: Option<String> = None;
    let mut current_device_id: Option<String> = None;
    let mut current_form_factor: Option<String> = None;
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
                     form_factor: Option<String>,
                     active_port: Option<String>,
                     ports_lines: &[String],
                     sinks: &mut Vec<SinkInfo>| {
        let port_refs: Vec<&str> = ports_lines.iter().map(|s| s.as_str()).collect();
        let active_port_available = parse_port_availability(&port_refs, active_port.as_deref());
        let ports = parse_ports_structured(ports_lines);
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
            form_factor,
            active_port,
            active_port_available,
            ports,
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
                    current_form_factor.take(),
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
            current_form_factor = None;
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
                    "device.form_factor" => current_form_factor = Some(value.to_string()),
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
            current_form_factor,
            current_active_port,
            &current_ports_lines,
            &mut sinks,
        );
    }

    Ok(sinks)
}

pub fn parse_sources(output: &str, default_source: Option<&str>) -> Result<Vec<SourceInfo>> {
    let mut sources = Vec::new();
    let mut current_name: Option<String> = None;
    let mut current_desc: Option<String> = None;
    let mut current_volume: f64 = 0.0;
    let mut current_muted: bool = false;
    let mut current_icon_name: Option<String> = None;
    let mut current_bus: Option<String> = None;
    let mut current_node_nick: Option<String> = None;
    let mut current_device_id: Option<String> = None;
    let mut current_form_factor: Option<String> = None;
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
                       form_factor: Option<String>,
                       active_port: Option<String>,
                       ports_lines: &[String],
                       sources: &mut Vec<SourceInfo>| {
        if name.contains(".monitor") {
            return;
        }
        let port_refs: Vec<&str> = ports_lines.iter().map(|s| s.as_str()).collect();
        let active_port_available = parse_port_availability(&port_refs, active_port.as_deref());
        let ports = parse_ports_structured(ports_lines);
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
            form_factor,
            active_port,
            active_port_available,
            ports,
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
                    current_form_factor.take(),
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
            current_form_factor = None;
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
                    "device.form_factor" => current_form_factor = Some(value.to_string()),
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
            current_form_factor,
            current_active_port,
            &current_ports_lines,
            &mut sources,
        );
    }

    Ok(sources)
}

/// Parse `pactl list cards` output to extract port-level product names.
pub fn parse_card_ports(output: &str) -> CardPortMap {
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

/// Parse `pactl list cards` output to extract card-level info (profiles, description, icon).
pub fn parse_cards(output: &str) -> Vec<CardInfo> {
    let mut cards = Vec::new();
    let mut current_name: Option<String> = None;
    let mut current_description: Option<String> = None;
    let mut current_icon_name: Option<String> = None;
    let mut current_bus: Option<String> = None;
    let mut current_form_factor: Option<String> = None;
    let mut current_profiles: Vec<CardProfile> = Vec::new();
    let mut current_active_profile: Option<String> = None;
    let mut section = ParseSection::Top;

    let push_card = |name: String,
                     description: String,
                     icon_name: Option<String>,
                     bus: Option<String>,
                     form_factor: Option<String>,
                     profiles: Vec<CardProfile>,
                     active_profile: String,
                     cards: &mut Vec<CardInfo>| {
        cards.push(CardInfo {
            name,
            description,
            icon_name,
            bus,
            form_factor,
            profiles,
            active_profile,
        });
    };

    for line in output.lines() {
        let trimmed = line.trim();

        if trimmed.starts_with("Card #") {
            // Save previous card
            if let (Some(name), Some(desc)) = (current_name.take(), current_description.take()) {
                push_card(
                    name,
                    desc,
                    current_icon_name.take(),
                    current_bus.take(),
                    current_form_factor.take(),
                    std::mem::take(&mut current_profiles),
                    current_active_profile.take().unwrap_or_default(),
                    &mut cards,
                );
            }
            current_icon_name = None;
            current_bus = None;
            current_form_factor = None;
            current_profiles.clear();
            current_active_profile = None;
            section = ParseSection::Top;
        } else if trimmed == "Properties:" {
            section = ParseSection::Properties;
        } else if trimmed == "Profiles:" || trimmed.starts_with("Profiles:") {
            section = ParseSection::Ports; // Reuse Ports section state for profile parsing
        } else if trimmed.starts_with("Active Profile:") {
            current_active_profile = Some(
                trimmed
                    .trim_start_matches("Active Profile:")
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
                    "device.form_factor" => current_form_factor = Some(value.to_string()),
                    "device.description" => {
                        current_description = Some(value.to_string());
                    }
                    _ => {}
                }
            }
        } else if section == ParseSection::Ports {
            // Profiles section (reusing Ports state)
            if !line.starts_with('\t') && !line.starts_with("    ") {
                section = ParseSection::Top;
            } else if trimmed.contains(':') && !trimmed.starts_with("Part of profile") {
                // Profile line like:
                // output:analog-stereo: Analog Stereo Output (sinks: 1, sources: 0, priority: 6500, available: yes)
                if let Some(colon_pos) = trimmed.find(": ") {
                    let profile_name = trimmed[..colon_pos].trim().to_string();
                    let rest = trimmed[colon_pos + 2..].trim();

                    // Extract description: text before first "("
                    let description = if let Some(paren_pos) = rest.find('(') {
                        rest[..paren_pos].trim().to_string()
                    } else {
                        rest.to_string()
                    };

                    // Parse available from parenthesized metadata
                    let available = if let Some(paren_start) = rest.find('(') {
                        let meta = &rest[paren_start..];
                        meta.contains("available: yes")
                    } else {
                        true
                    };

                    current_profiles.push(CardProfile {
                        name: profile_name,
                        description,
                        available,
                    });
                }
            }
        }

        // Top-level fields
        if section == ParseSection::Top {
            if trimmed.starts_with("Name:") {
                current_name = Some(trimmed.trim_start_matches("Name:").trim().to_string());
            } else if trimmed.starts_with("Description:") {
                // Top-level Description is always overridable by device.description in Properties
                if current_description.is_none() {
                    current_description = Some(
                        trimmed
                            .trim_start_matches("Description:")
                            .trim()
                            .to_string(),
                    );
                }
            }
        }
    }

    // Save last card
    if let (Some(name), Some(desc)) = (current_name, current_description) {
        push_card(
            name,
            desc,
            current_icon_name,
            current_bus,
            current_form_factor,
            current_profiles,
            current_active_profile.unwrap_or_default(),
            &mut cards,
        );
    }

    cards
}

/// Strip PipeWire-specific suffixes from a `device.icon_name` value.
///
/// PipeWire appends bus/connection suffixes like `-analog`, `-bluetooth`,
/// `-pci`, `-usb` to the base icon name (e.g. `"audio-card-analog"`).
/// This function removes known suffixes so the base name can be matched
/// against well-known icon categories.
fn strip_icon_suffix(name: &str) -> String {
    const SUFFIXES: &[&str] = &["-analog", "-bluetooth", "-pci", "-usb"];

    let mut result = name.to_string();
    for suffix in SUFFIXES {
        if let Some(stripped) = result.strip_suffix(suffix) {
            result = stripped.to_string();
            break;
        }
    }
    result
}

/// Compute the structured device type from PulseAudio metadata.
///
/// Priority: form_factor > port type > icon name fallback > input/output direction.
pub fn compute_device_type(
    form_factor: Option<&str>,
    icon_name: Option<&str>,
    port_type: Option<&str>,
    is_input: bool,
) -> String {
    // form_factor is the primary signal
    match form_factor {
        Some("headset") => return "headset".to_string(),
        Some("headphone") => return "headphone".to_string(),
        Some("hands-free") => return "hands-free".to_string(),
        Some("webcam") => return "webcam".to_string(),
        Some("phone") => return "phone".to_string(),
        Some("speaker") => return "speaker".to_string(),
        _ => {}
    }
    // HDMI/DisplayPort output -> display device
    if matches!(port_type, Some("HDMI") | Some("DisplayPort")) {
        return "display".to_string();
    }
    // GPU audio card identified by icon name (strips PipeWire bus suffixes before checking)
    if let Some(name) = icon_name {
        let base = strip_icon_suffix(name);
        if base == "video-display" {
            return "display".to_string();
        }
    }
    // Fall back to direction-based type
    if is_input {
        "microphone".to_string()
    } else {
        "card".to_string()
    }
}

/// Compute the primary icon for a sink.
///
/// First checks the active port name for hardware-specific hints (headphones,
/// headset, HDMI, speaker), then falls back to mapping well-known pactl
/// `device.icon_name` values to Adwaita icon names.
pub fn compute_primary_icon_sink(
    icon_name: &Option<String>,
    active_port: &Option<String>,
) -> String {
    // 1. Try to derive icon from active port name
    if let Some(port) = active_port {
        let port_lower = port.to_lowercase();
        if port_lower.contains("headphones") {
            return "audio-headphones-symbolic".to_string();
        }
        if port_lower.contains("headset") {
            return "audio-headphones-symbolic".to_string();
        }
        if port_lower.contains("hdmi") || port_lower.contains("displayport") {
            return "video-display-symbolic".to_string();
        }
        if port_lower.contains("speaker")
            || port_lower.contains("lineout")
            || port_lower.contains("line-out")
        {
            return "audio-speakers-symbolic".to_string();
        }
    }

    // 2. Map well-known icon names to Adwaita icons.
    // PipeWire may append suffixes like "-analog", "-bluetooth", "-pci" to the
    // base icon name (e.g. "audio-card-analog"). Strip known suffixes to get
    // the base name for matching.
    let base = icon_name.as_deref().map(strip_icon_suffix);
    match base.as_deref() {
        Some("audio-card") => "audio-speakers-symbolic".to_string(),
        Some("audio-headphones" | "audio-headset") => "audio-headphones-symbolic".to_string(),
        Some("video-display") => "video-display-symbolic".to_string(),
        Some(name) if !name.is_empty() => {
            if name.ends_with("-symbolic") {
                name.to_string()
            } else {
                format!("{name}-symbolic")
            }
        }
        _ => "audio-speakers-symbolic".to_string(),
    }
}

/// Compute the primary icon for a source.
///
/// First checks the active port name for hardware-specific hints (headset,
/// webcam), then falls back to mapping well-known pactl `device.icon_name`
/// values to Adwaita icon names.
pub fn compute_primary_icon_source(
    icon_name: &Option<String>,
    active_port: &Option<String>,
) -> String {
    // 1. Try to derive icon from active port name
    if let Some(port) = active_port {
        let port_lower = port.to_lowercase();
        if port_lower.contains("headset") {
            return "audio-headphones-symbolic".to_string();
        }
        if port_lower.contains("webcam") {
            return "camera-web-symbolic".to_string();
        }
    }

    // 2. Map well-known icon names to Adwaita icons.
    // PipeWire may append suffixes like "-analog", "-bluetooth", "-pci" to the
    // base icon name (e.g. "audio-card-analog"). Strip known suffixes to get
    // the base name for matching.
    let base = icon_name.as_deref().map(strip_icon_suffix);
    match base.as_deref() {
        Some("audio-card") => "audio-input-microphone-symbolic".to_string(),
        Some("camera-web") => "camera-web-symbolic".to_string(),
        Some("audio-headset") => "audio-headphones-symbolic".to_string(),
        Some(name) if !name.is_empty() => {
            if name.ends_with("-symbolic") {
                name.to_string()
            } else {
                format!("{name}-symbolic")
            }
        }
        _ => "audio-input-microphone-symbolic".to_string(),
    }
}

/// Compute the secondary icon based on device properties.
pub fn compute_secondary_icon(icon_name: &Option<String>, bus: &Option<String>) -> Option<String> {
    if icon_name.as_deref().map(strip_icon_suffix).as_deref() == Some("video-display") {
        Some("video-joined-displays-symbolic".to_string())
    } else if bus.as_deref() == Some("bluetooth") {
        Some("bluetooth-symbolic".to_string())
    } else {
        None
    }
}

/// Compute the display label for a device.
pub fn compute_label(
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
            && !product.is_empty()
        {
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
    if has_hdmi_secondary && let Some(stripped) = label.strip_prefix("HDMI / ") {
        label = stripped.to_string();
    }

    // 5. Strip "Bluetooth " prefix for Bluetooth devices
    if has_bt_secondary && let Some(stripped) = label.strip_prefix("Bluetooth ") {
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
        let icon = compute_primary_icon_sink(&sink.icon_name, &sink.active_port);
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
            volume: sink.volume_percent,
            muted: sink.muted,
        }
    }

    /// Create an AudioDevice from a SourceInfo with card port context.
    pub fn from_source(source: &SourceInfo, card_ports: &CardPortMap) -> Self {
        let icon = compute_primary_icon_source(&source.icon_name, &source.active_port);
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
            volume: source.volume_percent,
            muted: source.muted,
        }
    }
}

/// Compute the muted variant of an icon name.
pub fn muted_icon(base: &str) -> String {
    let stem = base.trim_end_matches("-symbolic");
    format!("{stem}-muted-symbolic")
}

#[cfg(test)]
#[path = "pactl_tests.rs"]
mod tests;
