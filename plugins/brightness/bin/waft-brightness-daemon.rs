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

use std::path::Path;
use std::sync::{Arc, Mutex as StdMutex, LazyLock};
use std::time::{Duration, Instant};

use anyhow::{Result, anyhow};
use log::{debug, info, warn};
use notify::{EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use std::process::Stdio;
use waft_plugin::*;

static I18N: LazyLock<waft_i18n::I18n> = LazyLock::new(|| waft_i18n::I18n::new(&[
    ("en-US", include_str!("../locales/en-US/brightness.ftl")),
    ("cs-CZ", include_str!("../locales/cs-CZ/brightness.ftl")),
]));

fn i18n() -> &'static waft_i18n::I18n { &I18N }

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
    connector: Option<String>,
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
        .map_err(|e| anyhow!("Failed to execute {program}: {e}"))
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
                id: format!("backlight:{device_name}"),
                name: display_name,
                display_type: DisplayType::Backlight,
                brightness,
                connector: None, // resolved later
            });
        }
    }

    Ok(displays)
}

// ---------------------------------------------------------------------------
// ddcutil backend
// ---------------------------------------------------------------------------

/// Parse DDC/CI monitors from `ddcutil detect` output (non-brief, for connector info).
async fn parse_ddc_monitors(ddcutil_output: &str) -> Result<Vec<Display>> {
    let mut displays = Vec::new();

    let mut current_display: Option<u32> = None;
    let mut current_model: Option<String> = None;

    for line in ddcutil_output.lines() {
        let trimmed = line.trim();

        if trimmed.starts_with("Display ") {
            // Flush previous display
            if let (Some(display_num), Some(model)) = (current_display.take(), current_model.take())
                && let Ok(brightness) = get_ddc_brightness(display_num).await
            {
                displays.push(Display {
                    id: format!("ddc:{display_num}"),
                    name: model,
                    display_type: DisplayType::External,
                    brightness,
                    connector: None, // resolved later
                });
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
    if let (Some(display_num), Some(model)) = (current_display, current_model)
        && let Ok(brightness) = get_ddc_brightness(display_num).await
    {
        displays.push(Display {
            id: format!("ddc:{display_num}"),
            name: model,
            display_type: DisplayType::External,
            brightness,
            connector: None, // resolved later
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
            .map_err(|_| anyhow!("Invalid DDC display number: {display_num}"))?;
        set_ddc_brightness(num, value).await
    } else {
        Err(anyhow!("Unknown device type: {device_id}"))
    }
}

async fn set_backlight_brightness(device_name: &str, value: f64) -> Result<()> {
    let percent = (value * 100.0).round() as u32;
    let output = run_command(
        "brightnessctl",
        &["-d", device_name, "set", &format!("{percent}%")],
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
// Sysfs connector extraction
// ---------------------------------------------------------------------------

/// Extract DRM connector name for a backlight device via sysfs traversal.
///
/// Path: `/sys/class/backlight/{device}/device` → PCI device → DRM connector.
/// For example, `intel_backlight` → PCI `0000:00:02.0` → `card1-eDP-1` → `eDP-1`.
fn resolve_backlight_connector(device_name: &str) -> Option<String> {
    let backlight_path = Path::new("/sys/class/backlight").join(device_name);
    let device_link = backlight_path.join("device");

    // Resolve the `device` symlink to the PCI device path
    let pci_path = match std::fs::canonicalize(&device_link) {
        Ok(p) => p,
        Err(e) => {
            debug!("[brightness] Could not resolve {}: {e}", device_link.display());
            return None;
        }
    };

    // Look for DRM connector directories under the PCI device's drm subdirectory.
    // Pattern: {pci_path}/drm/card*-{connector}
    let drm_dir = pci_path.join("drm");
    let entries = match std::fs::read_dir(&drm_dir) {
        Ok(e) => e,
        Err(e) => {
            debug!("[brightness] Could not read {}: {e}", drm_dir.display());
            return None;
        }
    };

    for entry in entries.flatten() {
        let name = entry.file_name();
        let name_str = name.to_string_lossy();
        // DRM entries look like "card0-eDP-1", "card1-DP-3", etc.
        // Strip the "cardN-" prefix to get the connector name.
        if let Some(pos) = name_str.find('-') {
            let connector = &name_str[pos + 1..];
            // Check if this is an enabled connector (has `enabled` file with "enabled")
            let status_path = entry.path().join("status");
            if let Ok(status) = std::fs::read_to_string(&status_path)
                && status.trim() == "connected"
            {
                debug!("[brightness] Backlight {device_name} → connector {connector}");
                return Some(connector.to_string());
            }
        }
    }

    // Fallback: if only one connector exists under the DRM device, use it
    let entries: Vec<_> = std::fs::read_dir(&drm_dir)
        .ok()?
        .flatten()
        .filter(|e| {
            let n = e.file_name();
            n.to_string_lossy().contains('-')
        })
        .collect();

    if entries.len() == 1 {
        let name = entries[0].file_name();
        let name_str = name.to_string_lossy();
        if let Some(pos) = name_str.find('-') {
            let connector = &name_str[pos + 1..];
            debug!("[brightness] Backlight {device_name} → connector {connector} (sole entry)");
            return Some(connector.to_string());
        }
    }

    debug!("[brightness] No connector found for backlight {device_name}");
    None
}

/// Extract DRM connector name for a DDC display by matching I2C bus to DRM `ddc` symlinks.
///
/// `ddcutil detect --brief` output includes `I2C bus:  /dev/i2c-N`. We match this bus number
/// against `/sys/class/drm/card*-*/ddc` symlinks which point to the I2C adapter.
fn resolve_ddc_connector(display_num: u32, ddcutil_output: &str) -> Option<String> {
    // First try parsing the I2C bus from cached ddcutil output
    let i2c_bus = parse_i2c_bus_for_display(display_num, ddcutil_output);

    if let Some(bus) = i2c_bus
        && let Some(connector) = match_i2c_bus_to_connector(bus)
    {
        return Some(connector);
    }

    debug!("[brightness] No connector found for DDC display {display_num}");
    None
}

/// Parse the I2C bus number for a given display from `ddcutil detect` output.
fn parse_i2c_bus_for_display(display_num: u32, output: &str) -> Option<u32> {
    let target = format!("Display {display_num}");
    let mut in_target_display = false;

    for line in output.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("Display ") {
            in_target_display = trimmed.starts_with(&target);
        }
        if in_target_display {
            // Look for "I2C bus:  /dev/i2c-N" or "I2C bus: /dev/i2c-N"
            if let Some(rest) = trimmed.strip_prefix("I2C bus:") {
                let rest = rest.trim();
                if let Some(num_str) = rest.strip_prefix("/dev/i2c-") {
                    return num_str.parse().ok();
                }
            }
        }
    }
    None
}

/// Match an I2C bus number to a DRM connector via `/sys/class/drm/card*-*/ddc` symlinks.
fn match_i2c_bus_to_connector(i2c_bus: u32) -> Option<String> {
    let drm_dir = Path::new("/sys/class/drm");
    let entries = std::fs::read_dir(drm_dir).ok()?;

    let target_suffix = format!("i2c-{i2c_bus}");

    for entry in entries.flatten() {
        let name = entry.file_name();
        let name_str = name.to_string_lossy();
        // Only look at connector entries (card0-DP-3, not card0)
        if !name_str.contains('-') {
            continue;
        }

        let ddc_path = entry.path().join("ddc");
        if let Ok(link_target) = std::fs::read_link(&ddc_path) {
            let link_str = link_target.to_string_lossy();
            if link_str.ends_with(&target_suffix) {
                // Extract connector from "card0-DP-3"
                if let Some(pos) = name_str.find('-') {
                    let connector = &name_str[pos + 1..];
                    debug!("[brightness] DDC i2c-{i2c_bus} → connector {connector}");
                    return Some(connector.to_string());
                }
            }
        }
    }
    None
}

// ---------------------------------------------------------------------------
// Backlight inotify watcher
// ---------------------------------------------------------------------------

/// Read current brightness from sysfs as a fraction (0.0 to 1.0).
fn read_sysfs_brightness(device_name: &str) -> Option<f64> {
    let base = Path::new("/sys/class/backlight").join(device_name);
    let actual = std::fs::read_to_string(base.join("actual_brightness")).ok()?;
    let max = std::fs::read_to_string(base.join("max_brightness")).ok()?;
    let actual: f64 = actual.trim().parse().ok()?;
    let max: f64 = max.trim().parse().ok()?;
    if max > 0.0 {
        Some(actual / max)
    } else {
        None
    }
}

/// Watch sysfs backlight files for external brightness changes.
///
/// Uses inotify (via `notify` crate) on `/sys/class/backlight/{device}/actual_brightness`.
/// On modify events, reads sysfs directly and updates shared state.
/// 50ms debounce coalesces rapid changes during key-repeat.
async fn watch_backlight_brightness(
    backlight_devices: Vec<String>,
    displays: Arc<StdMutex<Vec<Display>>>,
    notifier: EntityNotifier,
) -> Result<()> {
    let (tx, rx) = std::sync::mpsc::channel::<notify::Result<notify::Event>>();

    let mut watcher = RecommendedWatcher::new(tx, notify::Config::default())
        .map_err(|e| anyhow!("failed to create brightness watcher: {e}"))?;

    for device_name in &backlight_devices {
        let path = Path::new("/sys/class/backlight")
            .join(device_name)
            .join("actual_brightness");
        if path.exists() {
            if let Err(e) = watcher.watch(&path, RecursiveMode::NonRecursive) {
                warn!("[brightness] Could not watch {}: {e}", path.display());
            } else {
                debug!("[brightness] Watching {}", path.display());
            }
        }
    }

    tokio::task::spawn_blocking(move || {
        let _watcher = watcher; // Keep watcher alive
        let mut last_notify = Instant::now() - Duration::from_secs(1);
        let debounce = Duration::from_millis(50);

        for result in rx {
            match result {
                Ok(event) => {
                    if !matches!(event.kind, EventKind::Modify(_)) {
                        continue;
                    }

                    // Debounce: skip if too soon after last notification
                    let now = Instant::now();
                    if now.duration_since(last_notify) < debounce {
                        continue;
                    }

                    // Update brightness from sysfs for all backlight devices
                    let mut changed = false;
                    {
                        let mut state = displays.lock_or_recover();
                        for display in state.iter_mut() {
                            if display.display_type != DisplayType::Backlight {
                                continue;
                            }
                            let device_name = display
                                .id
                                .strip_prefix("backlight:")
                                .unwrap_or(&display.id);
                            if let Some(brightness) = read_sysfs_brightness(device_name)
                                && (display.brightness - brightness).abs() > 0.001
                            {
                                display.brightness = brightness;
                                changed = true;
                            }
                        }
                    }

                    if changed {
                        last_notify = now;
                        if !notifier.notify() {
                            debug!("[brightness] runtime stopped, exiting watcher");
                            break;
                        }
                        debug!("[brightness] brightness updated from sysfs");
                    }
                }
                Err(e) => {
                    warn!("[brightness] watcher error: {e}");
                }
            }
        }
        debug!("[brightness] watcher channel closed");
    })
    .await
    .map_err(|e| anyhow!("brightness watcher thread panicked: {e}"))?;

    Ok(())
}

// ---------------------------------------------------------------------------
// Daemon
// ---------------------------------------------------------------------------

struct BrightnessPlugin {
    displays: Arc<StdMutex<Vec<Display>>>,
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
            displays: Arc::new(StdMutex::new(displays)),
        })
    }

    /// Get a clone of the shared display state for background tasks.
    fn shared_displays(&self) -> Arc<StdMutex<Vec<Display>>> {
        self.displays.clone()
    }

    /// Discover all controllable displays from available backends.
    async fn discover_all() -> Vec<Display> {
        let mut displays = Vec::new();

        // Cache ddcutil output for both display discovery and connector resolution
        let ddcutil_output: Option<String> = if is_tool_available("ddcutil").await {
            match run_command("ddcutil", &["detect"]).await {
                Ok(output) if output.status.success() => {
                    Some(String::from_utf8_lossy(&output.stdout).to_string())
                }
                _ => None,
            }
        } else {
            None
        };

        if is_tool_available("brightnessctl").await {
            debug!("[brightness] brightnessctl is available");
            match discover_backlight_devices().await {
                Ok(devs) => {
                    debug!("[brightness] Found {} backlight device(s)", devs.len());
                    displays.extend(devs);
                }
                Err(e) => warn!("[brightness] Failed to discover backlight devices: {e}"),
            }
        } else {
            info!("[brightness] brightnessctl not available");
        }

        if let Some(ref output) = ddcutil_output {
            debug!("[brightness] ddcutil is available");
            match parse_ddc_monitors(output).await {
                Ok(devs) => {
                    debug!("[brightness] Found {} DDC monitor(s)", devs.len());
                    displays.extend(devs);
                }
                Err(e) => warn!("[brightness] Failed to discover DDC monitors: {e}"),
            }
        } else if !is_tool_available("ddcutil").await {
            info!("[brightness] ddcutil not available");
        }

        // Resolve connectors for all displays
        for display in &mut displays {
            display.connector = match display.display_type {
                DisplayType::Backlight => {
                    let device_name = display
                        .id
                        .strip_prefix("backlight:")
                        .unwrap_or(&display.id);
                    resolve_backlight_connector(device_name)
                }
                DisplayType::External => {
                    let display_num: u32 = display
                        .id
                        .strip_prefix("ddc:")
                        .and_then(|s| s.parse().ok())
                        .unwrap_or(0);
                    ddcutil_output
                        .as_deref()
                        .and_then(|out| resolve_ddc_connector(display_num, out))
                }
            };
        }

        // Sort: backlights first, then externals, alphabetically within each group
        displays.sort_by(|a, b| match (&a.display_type, &b.display_type) {
            (DisplayType::Backlight, DisplayType::External) => std::cmp::Ordering::Less,
            (DisplayType::External, DisplayType::Backlight) => std::cmp::Ordering::Greater,
            _ => a.name.cmp(&b.name),
        });

        displays
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
        let displays = self.displays.lock_or_recover();
        displays
            .iter()
            .map(|d| {
                let display = entity::display::Display {
                    name: d.name.clone(),
                    brightness: d.brightness,
                    kind: display_kind(d.display_type),
                    connector: d.connector.clone(),
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
    ) -> anyhow::Result<serde_json::Value> {
        if action == "set-brightness" {
            let new_brightness = params
                .get("value")
                .and_then(waft_plugin::serde_json::Value::as_f64)
                .unwrap_or(0.0)
                .clamp(0.0, 1.0);

            let device_id = urn.id().to_string();

            if let Err(e) = set_brightness(&device_id, new_brightness).await {
                log::error!(
                    "[brightness] Failed to set brightness for {device_id}: {e}"
                );
                return Err(e);
            }

            // Update local state
            {
                let mut displays = self.displays.lock_or_recover();
                if let Some(d) = displays.iter_mut().find(|d| d.id == device_id) {
                    d.brightness = new_brightness;
                }
            }
        } else {
            log::debug!("[brightness] Unknown action: {action}");
        }

        Ok(serde_json::Value::Null)
    }
}

// ---------------------------------------------------------------------------
// main
// ---------------------------------------------------------------------------

fn main() -> Result<()> {
    PluginRunner::new("brightness", &[entity::display::DISPLAY_ENTITY_TYPE])
        .i18n(i18n(), "plugin-name", "plugin-description")
        .run(|notifier| async {
            let plugin = BrightnessPlugin::new().await?;
            let shared = plugin.shared_displays();

            // Collect backlight device names for inotify watcher
            let backlight_devices: Vec<String> = {
                let displays = shared.lock_or_recover();
                displays
                    .iter()
                    .filter(|d| d.display_type == DisplayType::Backlight)
                    .map(|d| {
                        d.id.strip_prefix("backlight:")
                            .unwrap_or(&d.id)
                            .to_string()
                    })
                    .collect()
            };

            if !backlight_devices.is_empty() {
                spawn_monitored(
                    "brightness-watcher",
                    watch_backlight_brightness(backlight_devices, shared, notifier),
                );
            }

            Ok(plugin)
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
            connector: None,
        }
    }

    #[test]
    fn test_humanize_backlight_name_intel() {
        assert_eq!(
            humanize_backlight_name("intel_backlight"),
            "Built-in Display"
        );
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
            displays: Arc::new(StdMutex::new(Vec::new())),
        };
        assert!(plugin.get_entities().is_empty());
    }

    #[test]
    fn test_get_entities_single_display() {
        let plugin = BrightnessPlugin {
            displays: Arc::new(StdMutex::new(vec![test_display(
                "backlight:intel_backlight",
                0.5,
                DisplayType::Backlight,
            )])),
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
            displays: Arc::new(StdMutex::new(vec![
                test_display("backlight:intel_backlight", 0.5, DisplayType::Backlight),
                test_display("ddc:1", 0.8, DisplayType::External),
            ])),
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

    #[test]
    fn test_parse_i2c_bus_for_display() {
        let output = "\
Display 1
   I2C bus:  /dev/i2c-5
   Model: Dell U2723QE

Display 2
   I2C bus:  /dev/i2c-7
   Model: LG 27UK850";

        assert_eq!(parse_i2c_bus_for_display(1, output), Some(5));
        assert_eq!(parse_i2c_bus_for_display(2, output), Some(7));
        assert_eq!(parse_i2c_bus_for_display(3, output), None);
    }

    #[test]
    fn test_parse_i2c_bus_empty_output() {
        assert_eq!(parse_i2c_bus_for_display(1, ""), None);
    }

    #[test]
    fn test_parse_i2c_bus_single_space_format() {
        let output = "Display 1\n   I2C bus: /dev/i2c-3\n   Model: Test";
        assert_eq!(parse_i2c_bus_for_display(1, output), Some(3));
    }

    #[test]
    fn test_parse_i2c_bus_no_bus_line() {
        let output = "Display 1\n   Model: Test\n\nDisplay 2\n   I2C bus:  /dev/i2c-4";
        // Display 1 has no I2C bus line, should return None
        assert_eq!(parse_i2c_bus_for_display(1, output), None);
        // Display 2 has it
        assert_eq!(parse_i2c_bus_for_display(2, output), Some(4));
    }

    #[test]
    fn test_parse_i2c_bus_high_bus_number() {
        let output = "Display 1\n   I2C bus:  /dev/i2c-42";
        assert_eq!(parse_i2c_bus_for_display(1, output), Some(42));
    }

    #[test]
    fn test_get_entities_without_connector() {
        let plugin = BrightnessPlugin {
            displays: Arc::new(StdMutex::new(vec![test_display(
                "ddc:1",
                0.6,
                DisplayType::External,
            )])),
        };
        let entities = plugin.get_entities();
        let data: entity::display::Display =
            serde_json::from_value(entities[0].data.clone()).unwrap();
        assert_eq!(data.connector, None);
    }

    #[test]
    fn test_get_entities_with_connector() {
        let plugin = BrightnessPlugin {
            displays: Arc::new(StdMutex::new(vec![Display {
                id: "backlight:intel_backlight".to_string(),
                name: "Built-in Display".to_string(),
                display_type: DisplayType::Backlight,
                brightness: 0.7,
                connector: Some("eDP-1".to_string()),
            }])),
        };
        let entities = plugin.get_entities();
        let data: entity::display::Display =
            serde_json::from_value(entities[0].data.clone()).unwrap();
        assert_eq!(data.connector, Some("eDP-1".to_string()));
    }
}
