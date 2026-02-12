//! Brightness daemon -- display brightness control.
//!
//! Discovers controllable displays via `brightnessctl` (backlight) and `ddcutil` (DDC/CI external
//! monitors), then exposes one entity per display with brightness level and kind.
//!
//! Configuration (in ~/.config/waft/config.toml):
//! ```toml
//! [[plugins]]
//! id = "brightness"
//! ```

use anyhow::{Result, anyhow};
use log::{debug, info, warn};
use std::process::Stdio;
use waft_plugin::*;

// ---------------------------------------------------------------------------
// Display types
// ---------------------------------------------------------------------------

/// Type of display for backend selection.
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

// ---------------------------------------------------------------------------
// Daemon
// ---------------------------------------------------------------------------

struct BrightnessPlugin {
    displays: std::sync::Mutex<Vec<Display>>,
}

impl BrightnessPlugin {
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
        Ok(Self {
            displays: std::sync::Mutex::new(displays),
        })
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

    fn lock_displays(&self) -> std::sync::MutexGuard<'_, Vec<Display>> {
        match self.displays.lock() {
            Ok(g) => g,
            Err(e) => {
                warn!("[brightness] mutex poisoned, recovering: {e}");
                e.into_inner()
            }
        }
    }
}

fn display_kind(dt: DisplayType) -> entity::display::DisplayKind {
    match dt {
        DisplayType::Backlight => entity::display::DisplayKind::Backlight,
        DisplayType::External => entity::display::DisplayKind::External,
    }
}

#[async_trait::async_trait]
impl Plugin for BrightnessPlugin {
    fn get_entities(&self) -> Vec<Entity> {
        let displays = self.lock_displays();
        displays
            .iter()
            .map(|d| {
                let display = entity::display::Display {
                    name: d.name.clone(),
                    brightness: d.brightness,
                    kind: display_kind(d.display_type),
                };
                Entity::new(
                    Urn::new("brightness", entity::display::DISPLAY_ENTITY_TYPE, &d.id),
                    entity::display::DISPLAY_ENTITY_TYPE,
                    &display,
                )
            })
            .collect()
    }

    async fn handle_action(
        &self,
        urn: Urn,
        action: String,
        params: serde_json::Value,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        if action == "set-brightness" {
            let new_brightness = params
                .get("value")
                .and_then(|v| v.as_f64())
                .unwrap_or(0.0)
                .clamp(0.0, 1.0);

            let device_id = urn.id().to_string();

            if let Err(e) = set_brightness(&device_id, new_brightness).await {
                log::error!(
                    "[brightness] Failed to set brightness for {}: {}",
                    device_id,
                    e
                );
                return Err(e.into());
            }

            // Update local state
            {
                let mut displays = self.lock_displays();
                if let Some(d) = displays.iter_mut().find(|d| d.id == device_id) {
                    d.brightness = new_brightness;
                }
            }
        } else {
            log::debug!("[brightness] Unknown action: {}", action);
        }

        Ok(())
    }
}

// ---------------------------------------------------------------------------
// main
// ---------------------------------------------------------------------------

fn main() -> Result<()> {
    if waft_plugin::manifest::handle_provides(&[entity::display::DISPLAY_ENTITY_TYPE]) {
        return Ok(());
    }

    waft_plugin::init_plugin_logger("info");

    info!("Starting brightness plugin...");

    let rt = tokio::runtime::Runtime::new()?;
    rt.block_on(async {
        let plugin = BrightnessPlugin::new().await?;
        let (runtime, _notifier) = PluginRuntime::new("brightness", plugin);
        runtime.run().await?;
        Ok(())
    })
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
    fn test_get_entities_empty() {
        let plugin = BrightnessPlugin {
            displays: std::sync::Mutex::new(Vec::new()),
        };
        assert!(plugin.get_entities().is_empty());
    }

    #[test]
    fn test_get_entities_single_display() {
        let plugin = BrightnessPlugin {
            displays: std::sync::Mutex::new(vec![test_display(
                "backlight:intel_backlight",
                0.5,
                DisplayType::Backlight,
            )]),
        };
        let entities = plugin.get_entities();
        assert_eq!(entities.len(), 1);
        assert_eq!(
            entities[0].urn,
            Urn::new("brightness", "display", "backlight:intel_backlight")
        );
        assert_eq!(entities[0].entity_type, "display");

        let data: entity::display::Display =
            serde_json::from_value(entities[0].data.clone()).unwrap();
        assert_eq!(data.name, "backlight:intel_backlight");
        assert!((data.brightness - 0.5).abs() < 0.001);
        assert_eq!(data.kind, entity::display::DisplayKind::Backlight);
    }

    #[test]
    fn test_get_entities_multiple_displays() {
        let plugin = BrightnessPlugin {
            displays: std::sync::Mutex::new(vec![
                test_display("backlight:intel_backlight", 0.5, DisplayType::Backlight),
                test_display("ddc:1", 0.8, DisplayType::External),
            ]),
        };
        let entities = plugin.get_entities();
        assert_eq!(entities.len(), 2);

        let data0: entity::display::Display =
            serde_json::from_value(entities[0].data.clone()).unwrap();
        assert_eq!(data0.kind, entity::display::DisplayKind::Backlight);

        let data1: entity::display::Display =
            serde_json::from_value(entities[1].data.clone()).unwrap();
        assert_eq!(data1.kind, entity::display::DisplayKind::External);
    }
}
