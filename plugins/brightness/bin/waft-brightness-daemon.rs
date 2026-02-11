//! Brightness daemon -- display brightness control.
//!
//! Discovers controllable displays via `brightnessctl` (backlight) and `ddcutil` (DDC/CI external
//! monitors), then presents one or more brightness sliders to the user.
//!
//! - Single display: a single Slider widget.
//! - Multiple displays: a master Slider (average brightness, proportional scaling) with
//!   per-display Sliders inside an expandable container.

use anyhow::{Result, anyhow};
use log::{debug, info, warn};
use std::process::Stdio;
use waft_plugin_sdk::*;

// ---------------------------------------------------------------------------
// Display types
// ---------------------------------------------------------------------------

/// Type of display for icon selection.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DisplayType {
    /// Laptop/internal backlight (via brightnessctl)
    Backlight,
    /// External monitor (via ddcutil DDC/CI)
    External,
}

/// Information about a controllable display.
#[derive(Debug, Clone)]
struct Display {
    id: String,
    name: String,
    display_type: DisplayType,
    brightness: f64,
}

// ---------------------------------------------------------------------------
// Proportional scaling helpers
// ---------------------------------------------------------------------------

/// Compute the master brightness value (average of all displays).
fn compute_master_average(displays: &[Display]) -> f64 {
    if displays.is_empty() {
        return 0.0;
    }
    let sum: f64 = displays.iter().map(|d| d.brightness).sum();
    sum / displays.len() as f64
}

/// Apply proportional scaling to all displays based on master slider change.
///
/// Returns a vector of (display_id, new_brightness) tuples.
fn compute_proportional_scaling(
    displays: &[Display],
    old_master: f64,
    new_master: f64,
) -> Vec<(String, f64)> {
    if displays.is_empty() {
        return Vec::new();
    }

    // When old master is effectively zero, use additive -- set all to new_master.
    if old_master < 0.001 {
        return displays
            .iter()
            .map(|d| (d.id.clone(), new_master))
            .collect();
    }

    let ratio = new_master / old_master;
    displays
        .iter()
        .map(|d| {
            let new_brightness = (d.brightness * ratio).clamp(0.0, 1.0);
            (d.id.clone(), new_brightness)
        })
        .collect()
}

// ---------------------------------------------------------------------------
// CLI helpers
// ---------------------------------------------------------------------------

/// Run a CLI command and return its output.
async fn run_command(program: &str, args: &[&str]) -> Result<std::process::Output> {
    tokio::process::Command::new(program)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .await
        .map_err(|e| anyhow!("Failed to execute {}: {}", program, e))
}

/// Check if a CLI tool is available on PATH.
async fn is_tool_available(name: &str) -> bool {
    run_command("which", &[name])
        .await
        .map(|o| o.status.success())
        .unwrap_or(false)
}

// ---------------------------------------------------------------------------
// brightnessctl backend
// ---------------------------------------------------------------------------

/// Discover backlight devices using `brightnessctl -l -m -c backlight`.
///
/// Machine-readable format: device,class,current,percent,max
async fn discover_backlight_devices() -> Result<Vec<Display>> {
    let output = run_command("brightnessctl", &["-l", "-m", "-c", "backlight"]).await?;

    if !output.status.success() {
        return Ok(Vec::new());
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut displays = Vec::new();

    for line in stdout.lines() {
        if line.trim().is_empty() {
            continue;
        }

        let parts: Vec<&str> = line.split(',').collect();
        if parts.len() >= 5 {
            let device_name = parts[0];
            let current: u32 = parts[2].parse().unwrap_or(0);
            let max: u32 = parts[4].parse().unwrap_or(1);

            if max == 0 {
                continue;
            }

            let brightness = current as f64 / max as f64;
            let display_name = humanize_backlight_name(device_name);

            displays.push(Display {
                id: format!("backlight:{}", device_name),
                name: display_name,
                display_type: DisplayType::Backlight,
                brightness,
            });
        }
    }

    Ok(displays)
}

// ---------------------------------------------------------------------------
// ddcutil backend
// ---------------------------------------------------------------------------

/// Discover DDC/CI monitors using `ddcutil detect --brief`.
async fn discover_ddc_monitors() -> Result<Vec<Display>> {
    let output = run_command("ddcutil", &["detect", "--brief"]).await?;

    if !output.status.success() {
        return Ok(Vec::new());
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut displays = Vec::new();

    let mut current_display: Option<u32> = None;
    let mut current_model: Option<String> = None;

    for line in stdout.lines() {
        let trimmed = line.trim();

        if trimmed.starts_with("Display ") {
            // Flush previous display
            if let (Some(display_num), Some(model)) =
                (current_display.take(), current_model.take())
            {
                if let Ok(brightness) = get_ddc_brightness(display_num).await {
                    displays.push(Display {
                        id: format!("ddc:{}", display_num),
                        name: model,
                        display_type: DisplayType::External,
                        brightness,
                    });
                }
            }

            if let Some(num_str) = trimmed.strip_prefix("Display ") {
                current_display = num_str.trim().parse().ok();
                current_model = None;
            }
        } else if trimmed.starts_with("Model:") {
            current_model = Some(trimmed.trim_start_matches("Model:").trim().to_string());
        } else if current_model.is_none() && trimmed.contains("Monitor") {
            current_model = Some(trimmed.to_string());
        }
    }

    // Flush last display
    if let (Some(display_num), Some(model)) = (current_display, current_model) {
        if let Ok(brightness) = get_ddc_brightness(display_num).await {
            displays.push(Display {
                id: format!("ddc:{}", display_num),
                name: model,
                display_type: DisplayType::External,
                brightness,
            });
        }
    }

    Ok(displays)
}

/// Get current brightness for a DDC display (VCP code 0x10).
async fn get_ddc_brightness(display_num: u32) -> Result<f64> {
    let output = run_command(
        "ddcutil",
        &["getvcp", "10", "--brief", "-d", &display_num.to_string()],
    )
    .await?;

    if !output.status.success() {
        return Err(anyhow!("ddcutil getvcp failed"));
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    // Brief format: VCP 10 C 50 100
    for line in stdout.lines() {
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() >= 5 && parts[0] == "VCP" && parts[1] == "10" {
            let current: f64 = parts[3].parse().unwrap_or(0.0);
            let max: f64 = parts[4].parse().unwrap_or(100.0);
            if max > 0.0 {
                return Ok(current / max);
            }
        }
    }

    Err(anyhow!("Could not parse brightness from ddcutil output"))
}

// ---------------------------------------------------------------------------
// Brightness setter
// ---------------------------------------------------------------------------

/// Set brightness for a device identified by its id.
async fn set_brightness(device_id: &str, value: f64) -> Result<()> {
    let value = value.clamp(0.0, 1.0);

    if let Some(device_name) = device_id.strip_prefix("backlight:") {
        set_backlight_brightness(device_name, value).await
    } else if let Some(display_num) = device_id.strip_prefix("ddc:") {
        let num: u32 = display_num
            .parse()
            .map_err(|_| anyhow!("Invalid DDC display number: {}", display_num))?;
        set_ddc_brightness(num, value).await
    } else {
        Err(anyhow!("Unknown device type: {}", device_id))
    }
}

async fn set_backlight_brightness(device_name: &str, value: f64) -> Result<()> {
    let percent = (value * 100.0).round() as u32;
    let output = run_command(
        "brightnessctl",
        &["-d", device_name, "set", &format!("{}%", percent)],
    )
    .await?;

    if !output.status.success() {
        return Err(anyhow!(
            "brightnessctl set failed: {}",
            String::from_utf8_lossy(&output.stderr)
        ));
    }

    Ok(())
}

async fn set_ddc_brightness(display_num: u32, value: f64) -> Result<()> {
    let brightness = (value * 100.0).round() as u32;
    let output = run_command(
        "ddcutil",
        &[
            "setvcp",
            "10",
            &brightness.to_string(),
            "-d",
            &display_num.to_string(),
        ],
    )
    .await?;

    if !output.status.success() {
        return Err(anyhow!(
            "ddcutil setvcp failed: {}",
            String::from_utf8_lossy(&output.stderr)
        ));
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// Humanize helper
// ---------------------------------------------------------------------------

/// Convert a backlight device name to a human-readable name.
fn humanize_backlight_name(device_name: &str) -> String {
    if device_name.contains("intel")
        || device_name.contains("amdgpu")
        || device_name.contains("nvidia")
    {
        "Built-in Display".to_string()
    } else {
        device_name
            .replace('_', " ")
            .split_whitespace()
            .map(|w| {
                let mut c = w.chars();
                match c.next() {
                    None => String::new(),
                    Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
                }
            })
            .collect::<Vec<_>>()
            .join(" ")
    }
}

/// Return an appropriate icon name for a display type.
fn icon_for_display_type(dt: DisplayType) -> &'static str {
    match dt {
        DisplayType::Backlight => "display-brightness-symbolic",
        DisplayType::External => "video-display-symbolic",
    }
}

// ---------------------------------------------------------------------------
// Daemon
// ---------------------------------------------------------------------------

struct BrightnessDaemon {
    displays: Vec<Display>,
}

impl BrightnessDaemon {
    async fn new() -> Result<Self> {
        let displays = Self::discover_all().await;
        if displays.is_empty() {
            info!("[brightness] No controllable displays found");
        } else {
            info!(
                "[brightness] Found {} controllable display(s)",
                displays.len()
            );
        }
        Ok(Self { displays })
    }

    /// Discover all controllable displays from available backends.
    async fn discover_all() -> Vec<Display> {
        let mut displays = Vec::new();

        if is_tool_available("brightnessctl").await {
            debug!("[brightness] brightnessctl is available");
            match discover_backlight_devices().await {
                Ok(devs) => {
                    debug!("[brightness] Found {} backlight device(s)", devs.len());
                    displays.extend(devs);
                }
                Err(e) => warn!("[brightness] Failed to discover backlight devices: {}", e),
            }
        } else {
            info!("[brightness] brightnessctl not available");
        }

        if is_tool_available("ddcutil").await {
            debug!("[brightness] ddcutil is available");
            match discover_ddc_monitors().await {
                Ok(devs) => {
                    debug!("[brightness] Found {} DDC monitor(s)", devs.len());
                    displays.extend(devs);
                }
                Err(e) => warn!("[brightness] Failed to discover DDC monitors: {}", e),
            }
        } else {
            info!("[brightness] ddcutil not available");
        }

        // Sort: backlights first, then externals, alphabetically within each group
        displays.sort_by(|a, b| match (&a.display_type, &b.display_type) {
            (DisplayType::Backlight, DisplayType::External) => std::cmp::Ordering::Less,
            (DisplayType::External, DisplayType::Backlight) => std::cmp::Ordering::Greater,
            _ => a.name.cmp(&b.name),
        });

        displays
    }

    /// Build widget tree for the current state.
    fn build_widgets(&self) -> Widget {
        match self.displays.len() {
            0 => {
                // Empty label -- overview will ignore it
                Widget::Label {
                    text: String::new(),
                    css_classes: Vec::new(),
                }
            }
            1 => {
                // Single display -- simple slider
                let d = &self.displays[0];
                SliderBuilder::new(d.brightness)
                    .icon(icon_for_display_type(d.display_type))
                    .on_value_change("set_master")
                    .build()
            }
            _ => {
                // Multiple displays -- master slider with per-display sliders in expanded content
                let master = compute_master_average(&self.displays);

                let mut per_display_container =
                    ColBuilder::new().spacing(4);
                for d in &self.displays {
                    let slider = SliderBuilder::new(d.brightness)
                        .icon(icon_for_display_type(d.display_type))
                        .on_value_change(&format!("set_display:{}", d.id))
                        .build();
                    per_display_container = per_display_container.child(slider);
                }

                SliderBuilder::new(master)
                    .icon("display-brightness-symbolic")
                    .on_value_change("set_master")
                    .expandable(true)
                    .expanded_content(per_display_container.build())
                    .build()
            }
        }
    }
}

#[async_trait::async_trait]
impl PluginDaemon for BrightnessDaemon {
    fn get_widgets(&self) -> Vec<NamedWidget> {
        if self.displays.is_empty() {
            return Vec::new();
        }

        vec![NamedWidget {
            id: "brightness:control".to_string(),
            weight: 60,
            widget: self.build_widgets(),
        }]
    }

    async fn handle_action(
        &mut self,
        _widget_id: String,
        action: Action,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        if action.id == "set_master" {
            let new_master = match action.params {
                ActionParams::Value(v) => v,
                _ => return Ok(()),
            };

            let old_master = compute_master_average(&self.displays);
            let updates = compute_proportional_scaling(&self.displays, old_master, new_master);

            for (display_id, brightness) in &updates {
                if let Err(e) = set_brightness(display_id, *brightness).await {
                    log::error!(
                        "[brightness] Failed to set brightness for {}: {}",
                        display_id,
                        e
                    );
                }
            }

            // Update local state
            for (display_id, brightness) in updates {
                if let Some(d) = self.displays.iter_mut().find(|d| d.id == display_id) {
                    d.brightness = brightness;
                }
            }
        } else if let Some(target_id) = action.id.strip_prefix("set_display:") {
            let new_brightness = match action.params {
                ActionParams::Value(v) => v,
                _ => return Ok(()),
            };

            if let Err(e) = set_brightness(target_id, new_brightness).await {
                log::error!(
                    "[brightness] Failed to set brightness for {}: {}",
                    target_id,
                    e
                );
            }

            // Update local state
            if let Some(d) = self.displays.iter_mut().find(|d| d.id == target_id) {
                d.brightness = new_brightness;
            }
        }

        Ok(())
    }
}

// ---------------------------------------------------------------------------
// main
// ---------------------------------------------------------------------------

#[tokio::main]
async fn main() -> Result<()> {
    waft_plugin_sdk::init_daemon_logger("info");

    info!("Starting brightness daemon...");

    let daemon = BrightnessDaemon::new().await?;
    let (server, _notifier) = PluginServer::new("brightness-daemon", daemon);
    server.run().await?;

    Ok(())
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn test_display(id: &str, brightness: f64, dt: DisplayType) -> Display {
        Display {
            id: id.to_string(),
            name: id.to_string(),
            display_type: dt,
            brightness,
        }
    }

    #[test]
    fn test_compute_master_average_empty() {
        assert_eq!(compute_master_average(&[]), 0.0);
    }

    #[test]
    fn test_compute_master_average_single() {
        let displays = vec![test_display("a", 0.75, DisplayType::Backlight)];
        assert!((compute_master_average(&displays) - 0.75).abs() < 0.001);
    }

    #[test]
    fn test_compute_master_average_multiple() {
        let displays = vec![
            test_display("a", 0.5, DisplayType::Backlight),
            test_display("b", 0.9, DisplayType::External),
        ];
        assert!((compute_master_average(&displays) - 0.7).abs() < 0.001);
    }

    #[test]
    fn test_proportional_scaling_up() {
        let displays = vec![
            test_display("a", 0.25, DisplayType::Backlight),
            test_display("b", 0.45, DisplayType::External),
        ];
        let result = compute_proportional_scaling(&displays, 0.35, 0.70);
        assert_eq!(result.len(), 2);
        assert!((result[0].1 - 0.5).abs() < 0.001);
        assert!((result[1].1 - 0.9).abs() < 0.001);
    }

    #[test]
    fn test_proportional_scaling_to_zero() {
        let displays = vec![test_display("a", 0.5, DisplayType::Backlight)];
        let result = compute_proportional_scaling(&displays, 0.5, 0.0);
        assert_eq!(result[0].1, 0.0);
    }

    #[test]
    fn test_proportional_scaling_from_zero() {
        let displays = vec![
            test_display("a", 0.0, DisplayType::Backlight),
            test_display("b", 0.0, DisplayType::External),
        ];
        let result = compute_proportional_scaling(&displays, 0.0, 0.5);
        assert_eq!(result[0].1, 0.5);
        assert_eq!(result[1].1, 0.5);
    }

    #[test]
    fn test_proportional_scaling_clamps() {
        let displays = vec![test_display("a", 0.8, DisplayType::Backlight)];
        let result = compute_proportional_scaling(&displays, 0.4, 0.8);
        assert_eq!(result[0].1, 1.0);
    }

    #[test]
    fn test_humanize_backlight_name_intel() {
        assert_eq!(humanize_backlight_name("intel_backlight"), "Built-in Display");
    }

    #[test]
    fn test_humanize_backlight_name_amdgpu() {
        assert_eq!(humanize_backlight_name("amdgpu_bl0"), "Built-in Display");
    }

    #[test]
    fn test_humanize_backlight_name_generic() {
        assert_eq!(humanize_backlight_name("some_device"), "Some Device");
    }

    #[test]
    fn test_icon_for_display_type() {
        assert_eq!(
            icon_for_display_type(DisplayType::Backlight),
            "display-brightness-symbolic"
        );
        assert_eq!(
            icon_for_display_type(DisplayType::External),
            "video-display-symbolic"
        );
    }

    #[test]
    fn test_get_widgets_empty() {
        let daemon = BrightnessDaemon {
            displays: Vec::new(),
        };
        assert!(daemon.get_widgets().is_empty());
    }

    #[test]
    fn test_get_widgets_single_display() {
        let daemon = BrightnessDaemon {
            displays: vec![test_display("backlight:intel_backlight", 0.5, DisplayType::Backlight)],
        };
        let widgets = daemon.get_widgets();
        assert_eq!(widgets.len(), 1);
        assert_eq!(widgets[0].id, "brightness:control");
        assert_eq!(widgets[0].weight, 60);
        matches!(&widgets[0].widget, Widget::Slider { .. });
    }

    #[test]
    fn test_get_widgets_multiple_displays() {
        let daemon = BrightnessDaemon {
            displays: vec![
                test_display("backlight:intel_backlight", 0.5, DisplayType::Backlight),
                test_display("ddc:1", 0.8, DisplayType::External),
            ],
        };
        let widgets = daemon.get_widgets();
        assert_eq!(widgets.len(), 1);
        // Master slider with expanded content
        match &widgets[0].widget {
            Widget::Slider {
                expandable,
                expanded_content,
                ..
            } => {
                assert!(expandable);
                assert!(expanded_content.is_some());
            }
            other => panic!("Expected Slider, got {:?}", other),
        }
    }
}
