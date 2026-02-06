//! Brightness backend helpers.
//!
//! Interacts with brightnessctl and ddcutil CLI tools.

use anyhow::{Context, Result, anyhow};
use tokio::process::Command;

use crate::runtime::spawn_on_tokio;

use super::store::DisplayType;

/// Information about a discovered display.
#[derive(Debug, Clone)]
#[allow(dead_code)] // max_brightness is parsed but not yet consumed
pub struct DiscoveredDisplay {
    pub id: String,
    pub name: String,
    pub display_type: DisplayType,
    pub brightness: f64,
    pub max_brightness: u32,
}

/// Run a one-shot command on the tokio runtime.
async fn run_command(program: &str, args: &[&str]) -> Result<std::process::Output> {
    let program = program.to_string();
    let args: Vec<String> = args.iter().map(|s| s.to_string()).collect();
    spawn_on_tokio(async move {
        Command::new(&program)
            .args(&args)
            .output()
            .await
            .context(format!("Failed to execute {}", program))
    })
    .await
}

/// Check if brightnessctl CLI is available.
pub async fn is_brightnessctl_available() -> bool {
    run_command("which", &["brightnessctl"])
        .await
        .map(|o| o.status.success())
        .unwrap_or(false)
}

/// Check if ddcutil CLI is available.
pub async fn is_ddcutil_available() -> bool {
    run_command("which", &["ddcutil"])
        .await
        .map(|o| o.status.success())
        .unwrap_or(false)
}

/// Discover backlight devices using brightnessctl.
///
/// Uses `brightnessctl -l -m -c backlight` to enumerate backlight devices.
/// Machine-readable format: device,class,current,percent,max
pub async fn discover_backlight_devices() -> Result<Vec<DiscoveredDisplay>> {
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

        // Format: device,class,current,percent,max
        // Example: intel_backlight,backlight,1000,50%,2000
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

            displays.push(DiscoveredDisplay {
                id: format!("backlight:{}", device_name),
                name: display_name,
                display_type: DisplayType::Backlight,
                brightness,
                max_brightness: max,
            });
        }
    }

    Ok(displays)
}

/// Discover DDC/CI monitors using ddcutil.
///
/// Uses `ddcutil detect` to find monitors, then queries brightness for each.
pub async fn discover_ddc_monitors() -> Result<Vec<DiscoveredDisplay>> {
    let output = run_command("ddcutil", &["detect", "--brief"]).await?;

    if !output.status.success() {
        return Ok(Vec::new());
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut displays = Vec::new();

    // Parse ddcutil detect --brief output
    // Format varies, but typically includes display number and model
    let mut current_display: Option<u32> = None;
    let mut current_model: Option<String> = None;

    for line in stdout.lines() {
        let trimmed = line.trim();

        // Look for "Display N" lines
        if trimmed.starts_with("Display ") {
            // Save previous display if any
            if let (Some(display_num), Some(model)) = (current_display.take(), current_model.take())
                && let Ok(brightness) = get_ddc_brightness(display_num).await {
                    displays.push(DiscoveredDisplay {
                        id: format!("ddc:{}", display_num),
                        name: model,
                        display_type: DisplayType::External,
                        brightness,
                        max_brightness: 100,
                    });
                }

            // Parse new display number
            if let Some(num_str) = trimmed.strip_prefix("Display ") {
                current_display = num_str.trim().parse().ok();
                current_model = None;
            }
        } else if trimmed.starts_with("Model:") {
            current_model = Some(trimmed.trim_start_matches("Model:").trim().to_string());
        } else if current_model.is_none() && trimmed.contains("Monitor") {
            // Fallback: use any line mentioning Monitor as the name
            current_model = Some(trimmed.to_string());
        }
    }

    // Don't forget the last display
    if let (Some(display_num), Some(model)) = (current_display, current_model)
        && let Ok(brightness) = get_ddc_brightness(display_num).await {
            displays.push(DiscoveredDisplay {
                id: format!("ddc:{}", display_num),
                name: model,
                display_type: DisplayType::External,
                brightness,
                max_brightness: 100,
            });
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
    // (feature code, type, current value, max value)
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

/// Get brightness for a device.
#[allow(dead_code)] // Available for future use but discover_displays is currently used instead
pub async fn get_brightness(device_id: &str) -> Result<f64> {
    if let Some(device_name) = device_id.strip_prefix("backlight:") {
        get_backlight_brightness(device_name).await
    } else if let Some(display_num) = device_id.strip_prefix("ddc:") {
        let num: u32 = display_num.parse()?;
        get_ddc_brightness(num).await
    } else {
        Err(anyhow!("Unknown device type: {}", device_id))
    }
}

/// Get brightness for a backlight device.
async fn get_backlight_brightness(device_name: &str) -> Result<f64> {
    let output = run_command("brightnessctl", &["-m", "-d", device_name, "get"]).await?;

    if !output.status.success() {
        return Err(anyhow!("brightnessctl get failed"));
    }

    let current: u32 = String::from_utf8_lossy(&output.stdout)
        .trim()
        .parse()
        .unwrap_or(0);

    // Get max
    let max_output = run_command("brightnessctl", &["-m", "-d", device_name, "max"]).await?;
    let max: u32 = String::from_utf8_lossy(&max_output.stdout)
        .trim()
        .parse()
        .unwrap_or(1);

    if max == 0 {
        return Ok(0.0);
    }

    Ok(current as f64 / max as f64)
}

/// Set brightness for a device.
pub async fn set_brightness(device_id: &str, value: f64) -> Result<()> {
    let value = value.clamp(0.0, 1.0);

    if let Some(device_name) = device_id.strip_prefix("backlight:") {
        set_backlight_brightness(device_name, value).await
    } else if let Some(display_num) = device_id.strip_prefix("ddc:") {
        let num: u32 = display_num.parse()?;
        set_ddc_brightness(num, value).await
    } else {
        Err(anyhow!("Unknown device type: {}", device_id))
    }
}

/// Set brightness for a backlight device.
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

/// Set brightness for a DDC display.
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

/// Convert a backlight device name to a human-readable name.
fn humanize_backlight_name(device_name: &str) -> String {
    // Common patterns: intel_backlight, amdgpu_bl0, nvidia_0, etc.
    if device_name.contains("intel")
        || device_name.contains("amdgpu")
        || device_name.contains("nvidia")
    {
        "Built-in Display".to_string()
    } else {
        // Fallback: capitalize and clean up
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

#[cfg(test)]
#[path = "dbus_tests.rs"]
mod tests;
