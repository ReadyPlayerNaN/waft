//! awww plugin -- wallpaper management via awww or swww CLI.
//!
//! Provides `wallpaper-manager` entities for each display output and a
//! synthetic "all" entity for synchronized mode. Wraps `awww query` and
//! `awww img` commands (falls back to `swww` if `awww` is not installed).
//!
//! Supports three operational modes:
//! - **Static**: Pick a wallpaper from the configured directory; it stays until changed.
//! - **StyleTracking**: Follows dark/light mode via darkman D-Bus signals.
//! - **DayTracking**: Follows time of day using six day segments (no polling).
//!
//! Configuration (in ~/.config/waft/config.toml):
//! ```toml
//! [[plugins]]
//! id = "awww"
//! wallpaper_dir = "~/.config/waft/wallpapers"
//! sync = true
//! mode = "static"   # "static" | "style-tracking" | "day-tracking"
//!
//! [plugins.transition]
//! transition_type = "fade"
//! fps = 60
//! angle = 0
//! duration = 1.0
//! ```

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::LazyLock;
use std::sync::{Arc, Mutex as StdMutex};
use std::time::Duration;

use anyhow::{Context, Result};
use chrono::{Local, Timelike};
use serde::Deserialize;
use waft_plugin::StateLocker;
use waft_plugin::dbus_monitor::{SignalMonitorConfig, monitor_signal};
use waft_plugin::*;
use zbus::Connection;

use waft_protocol::entity::display::{
    DaySegment, WallpaperManager, WallpaperMode, WallpaperTransition,
    WALLPAPER_MANAGER_ENTITY_TYPE,
};

static I18N: LazyLock<waft_i18n::I18n> = LazyLock::new(|| waft_i18n::I18n::new(&[
    ("en-US", include_str!("../locales/en-US/awww.ftl")),
    ("cs-CZ", include_str!("../locales/cs-CZ/awww.ftl")),
]));

fn i18n() -> &'static waft_i18n::I18n { &I18N }

const DARKMAN_DESTINATION: &str = "nl.whynothugo.darkman";
const DARKMAN_PATH: &str = "/nl/whynothugo/darkman";
const DARKMAN_INTERFACE: &str = "nl.whynothugo.darkman";

/// Plugin configuration persisted to config.toml.
#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
struct AwwwConfig {
    #[allow(dead_code)]
    id: String,
    wallpaper_dir: String,
    sync: bool,
    transition: TransitionConfig,
    mode: String,
}

impl Default for AwwwConfig {
    fn default() -> Self {
        Self {
            id: "awww".to_string(),
            wallpaper_dir: "~/.config/waft/wallpapers".to_string(),
            sync: true,
            transition: TransitionConfig::default(),
            mode: "static".to_string(),
        }
    }
}

/// Transition configuration.
#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
struct TransitionConfig {
    transition_type: String,
    fps: u32,
    angle: u32,
    duration: f64,
}

impl Default for TransitionConfig {
    fn default() -> Self {
        Self {
            transition_type: "fade".to_string(),
            fps: 60,
            angle: 0,
            duration: 1.0,
        }
    }
}

/// Commands sent to the wallpaper applicator task.
enum WallpaperCommand {
    /// Apply a random wallpaper from the given directory.
    ApplyFromDir { dir: String },
}

/// Shared plugin state.
struct AwwwState {
    /// The CLI backend in use ("awww" or "swww").
    backend: String,
    /// Per-output current wallpaper paths (from query).
    outputs: HashMap<String, Option<String>>,
    /// Active transition config.
    transition: TransitionConfig,
    /// Wallpaper directory (unexpanded, as stored in config).
    wallpaper_dir: String,
    /// Sync mode.
    sync: bool,
    /// Whether the backend is available.
    available: bool,
    /// Active wallpaper mode.
    mode: WallpaperMode,
    /// Current day segment (tracked even when not in DayTracking mode).
    current_segment: Option<DaySegment>,
    /// Whether style tracking mode is available (darkman D-Bus reachable).
    style_tracking_available: bool,
    /// Current dark mode state from darkman (None = unknown).
    dark_mode_active: Option<bool>,
}

fn lock_state(state: &StdMutex<AwwwState>) -> std::sync::MutexGuard<'_, AwwwState> {
    state.lock_or_recover()
}

/// Detect which wallpaper CLI backend is available. Prefers `awww`, falls back to `swww`.
fn detect_backend() -> String {
    for candidate in ["awww", "swww"] {
        // A successful spawn (even if the command fails) means the binary exists.
        if std::process::Command::new(candidate)
            .arg("--version")
            .output()
            .is_ok()
        {
            return candidate.to_string();
        }
    }
    "awww".to_string()
}

/// Parse a mode string into WallpaperMode.
fn parse_mode(s: &str) -> WallpaperMode {
    match s {
        "style-tracking" => WallpaperMode::StyleTracking,
        "day-tracking" => WallpaperMode::DayTracking,
        _ => WallpaperMode::Static,
    }
}

/// Serialize WallpaperMode to config string.
fn mode_to_str(mode: WallpaperMode) -> &'static str {
    match mode {
        WallpaperMode::Static => "static",
        WallpaperMode::StyleTracking => "style-tracking",
        WallpaperMode::DayTracking => "day-tracking",
    }
}

/// The awww plugin.
struct AwwwPlugin {
    state: Arc<StdMutex<AwwwState>>,
    wp_tx: tokio::sync::mpsc::Sender<WallpaperCommand>,
}

impl AwwwPlugin {
    async fn new(wp_tx: tokio::sync::mpsc::Sender<WallpaperCommand>) -> Result<Self> {
        // Load config: try "awww" first (new), fall back to "swww" (migration).
        let config: AwwwConfig = waft_plugin::config::load_plugin_config("awww")
            .or_else(|_| waft_plugin::config::load_plugin_config("swww"))
            .unwrap_or_default();

        let mode = parse_mode(&config.mode);
        let backend = detect_backend();

        // Attempt init best-effort
        if let Err(e) = run_init(&backend).await {
            log::debug!("[awww] {backend} init failed (best-effort): {e}");
        }

        // Query current state
        let (available, outputs) = match run_query(&backend).await {
            Ok(outputs) => (true, outputs),
            Err(e) => {
                log::warn!("[awww] {backend} query failed, marking unavailable: {e}");
                (false, HashMap::new())
            }
        };

        // Compute initial day segment
        let now = Local::now();
        let current_segment = Some(DaySegment::from_time(now.hour(), now.minute()));

        log::info!(
            "[awww] Plugin started: backend={backend}, available={available}, outputs={}, mode={:?}",
            outputs.len(),
            mode,
        );

        Ok(Self {
            state: Arc::new(StdMutex::new(AwwwState {
                backend,
                outputs,
                transition: config.transition,
                wallpaper_dir: config.wallpaper_dir,
                sync: config.sync,
                available,
                mode,
                current_segment,
                style_tracking_available: false,
                dark_mode_active: None,
            })),
            wp_tx,
        })
    }

    fn lock_state(&self) -> std::sync::MutexGuard<'_, AwwwState> {
        lock_state(&self.state)
    }

    /// Build a WallpaperTransition from the current config.
    fn transition_from_config(config: &TransitionConfig) -> WallpaperTransition {
        WallpaperTransition {
            transition_type: config.transition_type.clone(),
            fps: config.fps,
            angle: config.angle,
            duration: config.duration,
        }
    }

    /// Expand `~` prefix in a path to the user's home directory.
    fn expand_tilde(path: &str) -> String {
        if let Some(rest) = path.strip_prefix("~/")
            && let Some(home) = dirs::home_dir()
        {
            return home.join(rest).to_string_lossy().to_string();
        } else if path == "~"
            && let Some(home) = dirs::home_dir()
        {
            return home.to_string_lossy().to_string();
        }
        path.to_string()
    }

    /// Refresh outputs by re-running query.
    async fn refresh_state(&self) {
        let backend = self.lock_state().backend.clone();
        match run_query(&backend).await {
            Ok(outputs) => {
                let mut state = self.lock_state();
                state.outputs = outputs;
                state.available = true;
            }
            Err(e) => {
                log::warn!("[awww] refresh query failed: {e}");
                let mut state = self.lock_state();
                state.available = false;
            }
        }
    }

    /// Compute the active wallpaper directory based on mode.
    fn active_wallpaper_dir(state: &AwwwState) -> String {
        let base = Self::expand_tilde(&state.wallpaper_dir);
        match state.mode {
            WallpaperMode::Static => base,
            WallpaperMode::StyleTracking => {
                let subfolder = match state.dark_mode_active {
                    Some(true) => "dark",
                    _ => "light",
                };
                format!("{base}/{subfolder}")
            }
            WallpaperMode::DayTracking => {
                let segment = state.current_segment.unwrap_or_else(|| {
                    let now = Local::now();
                    DaySegment::from_time(now.hour(), now.minute())
                });
                format!("{}/{}", base, segment.folder_name())
            }
        }
    }

    /// Persist the current config to config.toml.
    fn persist_config(&self) {
        let state = self.lock_state();
        let config_path = match dirs::config_dir() {
            Some(d) => d.join("waft/config.toml"),
            None => {
                log::warn!("[awww] cannot determine config directory for persistence");
                return;
            }
        };

        if let Err(e) = persist_awww_config(
            &config_path,
            &state.wallpaper_dir,
            state.sync,
            &state.transition,
            state.mode,
        ) {
            log::error!("[awww] failed to persist config: {e}");
        }
    }
}

#[async_trait::async_trait]
impl Plugin for AwwwPlugin {
    fn get_entities(&self) -> Vec<Entity> {
        let state = self.lock_state();
        let transition = Self::transition_from_config(&state.transition);

        let mut entities = Vec::new();

        // Per-output entities
        for (output, wallpaper) in &state.outputs {
            let manager = WallpaperManager {
                output: output.clone(),
                current_wallpaper: wallpaper.clone(),
                available: state.available,
                transition: transition.clone(),
                wallpaper_dir: state.wallpaper_dir.clone(),
                sync: state.sync,
                mode: state.mode,
                current_segment: state.current_segment,
                style_tracking_available: state.style_tracking_available,
            };
            entities.push(Entity::new(
                Urn::new("awww", WALLPAPER_MANAGER_ENTITY_TYPE, output),
                WALLPAPER_MANAGER_ENTITY_TYPE,
                &manager,
            ));
        }

        // Synthetic "all" entity
        let all_wallpaper = if state.sync {
            state.outputs.values().next().cloned().flatten()
        } else {
            None
        };

        let all_manager = WallpaperManager {
            output: "all".to_string(),
            current_wallpaper: all_wallpaper,
            available: state.available,
            transition: transition.clone(),
            wallpaper_dir: state.wallpaper_dir.clone(),
            sync: state.sync,
            mode: state.mode,
            current_segment: state.current_segment,
            style_tracking_available: state.style_tracking_available,
        };
        entities.push(Entity::new(
            Urn::new("awww", WALLPAPER_MANAGER_ENTITY_TYPE, "all"),
            WALLPAPER_MANAGER_ENTITY_TYPE,
            &all_manager,
        ));

        entities
    }

    async fn handle_action(
        &self,
        urn: Urn,
        action: String,
        params: serde_json::Value,
    ) -> anyhow::Result<serde_json::Value> {
        let output_id = urn.id().to_string();

        match action.as_str() {
            "set-wallpaper" => {
                let path = params
                    .get("path")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| anyhow::anyhow!("missing 'path' parameter"))?;

                let (backend, transition, sync, targets): (String, TransitionConfig, bool, Option<String>) = {
                    let state = self.lock_state();
                    let targets = if sync_applies(&output_id, state.sync) {
                        None
                    } else {
                        Some(output_id.clone())
                    };
                    (state.backend.clone(), state.transition.clone(), state.sync, targets)
                };

                run_img(&backend, path, targets.as_deref(), &transition).await?;

                if sync {
                    log::debug!("[awww] set-wallpaper in sync mode, applied to all outputs");
                }

                self.refresh_state().await;
            }

            "random" => {
                let (backend, dir, transition, sync) = {
                    let state = self.lock_state();
                    (
                        state.backend.clone(),
                        Self::active_wallpaper_dir(&state),
                        state.transition.clone(),
                        state.sync,
                    )
                };

                let path = pick_random_wallpaper(&dir)?;
                let path_str = path.to_string_lossy();

                let targets = if sync_applies(&output_id, sync) {
                    None
                } else {
                    Some(output_id.as_str())
                };

                run_img(&backend, &path_str, targets, &transition).await?;
                self.refresh_state().await;
            }

            "set-mode" => {
                let mode_str = params
                    .get("mode")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| anyhow::anyhow!("missing 'mode' parameter"))?;
                let new_mode = match mode_str {
                    "static" => WallpaperMode::Static,
                    "style-tracking" => WallpaperMode::StyleTracking,
                    "day-tracking" => WallpaperMode::DayTracking,
                    _ => anyhow::bail!("unknown mode: {mode_str}"),
                };

                {
                    let mut s = self.lock_state();
                    if new_mode == WallpaperMode::StyleTracking && !s.style_tracking_available {
                        anyhow::bail!("style-tracking unavailable: darkman not running");
                    }
                    s.mode = new_mode;
                }

                // Apply wallpaper for the new mode immediately
                match new_mode {
                    WallpaperMode::Static => { /* no auto-change */ }
                    WallpaperMode::StyleTracking => {
                        let dir = {
                            let state = self.lock_state();
                            if state.dark_mode_active.is_some() {
                                Some(Self::active_wallpaper_dir(&state))
                            } else {
                                None
                            }
                        };
                        if let Some(dir) = dir
                            && let Err(e) = self.wp_tx.try_send(WallpaperCommand::ApplyFromDir { dir })
                        {
                            log::warn!("[awww] failed to queue style-tracking wallpaper: {e}");
                        }
                    }
                    WallpaperMode::DayTracking => {
                        let dir = {
                            let state = self.lock_state();
                            Self::active_wallpaper_dir(&state)
                        };
                        if let Err(e) = self.wp_tx.try_send(WallpaperCommand::ApplyFromDir { dir }) {
                            log::warn!("[awww] failed to queue day-tracking wallpaper: {e}");
                        }
                    }
                }

                self.persist_config();
            }

            "update-transition" => {
                {
                    let mut state = self.lock_state();
                    if let Some(t) = params.get("transition_type").and_then(|v| v.as_str()) {
                        state.transition.transition_type = t.to_string();
                    }
                    if let Some(fps) = params.get("fps").and_then(waft_plugin::serde_json::Value::as_u64) {
                        state.transition.fps = fps as u32;
                    }
                    if let Some(angle) = params.get("angle").and_then(waft_plugin::serde_json::Value::as_u64) {
                        state.transition.angle = angle as u32;
                    }
                    if let Some(duration) = params.get("duration").and_then(waft_plugin::serde_json::Value::as_f64) {
                        state.transition.duration = duration;
                    }
                }
                self.persist_config();
            }

            "update-config" => {
                {
                    let mut state = self.lock_state();
                    if let Some(dir) = params.get("wallpaper_dir").and_then(|v| v.as_str()) {
                        state.wallpaper_dir = dir.to_string();
                    }
                    if let Some(sync) = params.get("sync").and_then(waft_plugin::serde_json::Value::as_bool) {
                        state.sync = sync;
                    }
                }
                self.persist_config();
            }

            other => {
                log::debug!("[awww] Unknown action: {other}");
            }
        }

        Ok(serde_json::Value::Null)
    }

    fn can_stop(&self) -> bool {
        true
    }
}

/// Whether actions should apply to all outputs (no --outputs flag).
fn sync_applies(output_id: &str, sync: bool) -> bool {
    output_id == "all" || sync
}

/// Run `<backend> init` best-effort to ensure the daemon is started.
async fn run_init(backend: &str) -> Result<()> {
    let output = tokio::process::Command::new(backend)
        .arg("init")
        .output()
        .await
        .with_context(|| format!("failed to run {backend} init"))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        // "already running" is not an error
        if !stderr.contains("already running") {
            anyhow::bail!("{backend} init failed: {stderr}");
        }
    }
    Ok(())
}

/// Run `<backend> query` and parse the output into a map of output -> wallpaper path.
async fn run_query(backend: &str) -> Result<HashMap<String, Option<String>>> {
    let output = tokio::process::Command::new(backend)
        .arg("query")
        .output()
        .await
        .with_context(|| format!("failed to run {backend} query"))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("{backend} query failed: {stderr}");
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut outputs = HashMap::new();

    for line in stdout.lines() {
        // Format: "OUTPUT_NAME: ..., currently displaying: image: /path/to/image"
        // or "OUTPUT_NAME: ..." without the image part
        if let Some((output_name, rest)) = line.split_once(':') {
            let output_name = output_name.trim().to_string();
            let wallpaper = if let Some(img_idx) = rest.find("currently displaying: image: ") {
                let path_start = img_idx + "currently displaying: image: ".len();
                let path = rest[path_start..].trim();
                if path.is_empty() {
                    None
                } else {
                    Some(path.to_string())
                }
            } else {
                None
            };
            outputs.insert(output_name, wallpaper);
        }
    }

    Ok(outputs)
}

/// Run `<backend> img` to set wallpaper.
async fn run_img(
    backend: &str,
    path: &str,
    output: Option<&str>,
    transition: &TransitionConfig,
) -> anyhow::Result<()> {
    let mut cmd = tokio::process::Command::new(backend);
    cmd.arg("img").arg(path);

    if let Some(output_name) = output {
        cmd.args(["--outputs", output_name]);
    }

    cmd.args([
        "--transition-type",
        &transition.transition_type,
        "--transition-fps",
        &transition.fps.to_string(),
        "--transition-angle",
        &transition.angle.to_string(),
        "--transition-duration",
        &transition.duration.to_string(),
    ]);

    let result = cmd.output().await
        .map_err(|e| anyhow::anyhow!("failed to run {backend} img: {e}"))?;

    if !result.status.success() {
        let stderr = String::from_utf8_lossy(&result.stderr);
        anyhow::bail!("{backend} img failed: {stderr}");
    }

    Ok(())
}

/// Pick a random wallpaper from the given directory.
fn pick_random_wallpaper(dir: &str) -> anyhow::Result<PathBuf> {
    let dir_path = Path::new(dir);
    if !dir_path.exists() {
        anyhow::bail!("Wallpaper directory does not exist: {dir}");
    }

    let extensions = ["png", "jpg", "jpeg", "webp", "gif", "bmp"];
    let mut candidates = Vec::new();

    let entries = std::fs::read_dir(dir_path)
        .map_err(|e| anyhow::anyhow!("Failed to read wallpaper directory: {e}"))?;

    for entry in entries {
        let entry = match entry {
            Ok(e) => e,
            Err(e) => {
                log::debug!("[awww] skipping dir entry: {e}");
                continue;
            }
        };
        let path = entry.path();
        if path.is_file()
            && let Some(ext) = path.extension().and_then(|e| e.to_str())
            && extensions.contains(&ext.to_lowercase().as_str())
        {
            candidates.push(path);
        }
    }

    if candidates.is_empty() {
        anyhow::bail!("No wallpaper files found in {dir}");
    }

    let idx = fastrand::usize(..candidates.len());
    Ok(candidates.swap_remove(idx))
}

/// Persist awww config to the TOML config file using read-modify-write.
/// Writes as `id = "awww"`. Migrates existing `id = "swww"` entries on first write.
fn persist_awww_config(
    config_path: &Path,
    wallpaper_dir: &str,
    sync: bool,
    transition: &TransitionConfig,
    mode: WallpaperMode,
) -> Result<()> {
    let content = if config_path.exists() {
        std::fs::read_to_string(config_path).unwrap_or_default()
    } else {
        String::new()
    };

    let mut root: toml::Table = toml::from_str(&content).unwrap_or_default();

    // Find or create the awww plugin entry (also migrates from "swww" id)
    let plugins = root
        .entry("plugins")
        .or_insert_with(|| toml::Value::Array(Vec::new()));

    if let toml::Value::Array(arr) = plugins {
        // Find existing entry by "awww" or legacy "swww" id
        let existing = arr.iter_mut().find(|p| {
            p.as_table()
                .and_then(|t| t.get("id"))
                .and_then(|v| v.as_str())
                .map(|id| id == "awww" || id == "swww")
                .unwrap_or(false)
        });

        let table = if let Some(entry) = existing {
            entry.as_table_mut().expect("plugin entry must be table")
        } else {
            let mut new_table = toml::Table::new();
            new_table.insert("id".to_string(), toml::Value::String("awww".to_string()));
            arr.push(toml::Value::Table(new_table));
            arr.last_mut()
                .expect("just pushed element")
                .as_table_mut()
                .expect("just inserted")
        };

        // Always write canonical id
        table.insert("id".to_string(), toml::Value::String("awww".to_string()));
        table.insert(
            "wallpaper_dir".to_string(),
            toml::Value::String(wallpaper_dir.to_string()),
        );
        table.insert("sync".to_string(), toml::Value::Boolean(sync));
        table.insert(
            "mode".to_string(),
            toml::Value::String(mode_to_str(mode).to_string()),
        );

        let mut transition_table = toml::Table::new();
        transition_table.insert(
            "transition_type".to_string(),
            toml::Value::String(transition.transition_type.clone()),
        );
        transition_table.insert(
            "fps".to_string(),
            toml::Value::Integer(i64::from(transition.fps)),
        );
        transition_table.insert(
            "angle".to_string(),
            toml::Value::Integer(i64::from(transition.angle)),
        );
        transition_table.insert(
            "duration".to_string(),
            toml::Value::Float(transition.duration),
        );
        table.insert(
            "transition".to_string(),
            toml::Value::Table(transition_table),
        );
    }

    // Ensure parent directory exists
    if let Some(parent) = config_path.parent() {
        std::fs::create_dir_all(parent).context("failed to create config directory")?;
    }

    let serialized = toml::to_string_pretty(&root).context("failed to serialize config")?;
    std::fs::write(config_path, serialized).context("failed to write config file")?;

    log::debug!("[awww] Config persisted to {}", config_path.display());
    Ok(())
}

/// Get darkman mode via D-Bus property.
async fn get_darkman_mode(conn: &Connection) -> Result<bool> {
    let proxy = zbus::Proxy::new(
        conn,
        DARKMAN_DESTINATION,
        DARKMAN_PATH,
        "org.freedesktop.DBus.Properties",
    )
    .await
    .context("failed to create D-Bus proxy")?;

    let (value,): (zbus::zvariant::OwnedValue,) = proxy
        .call("Get", &(DARKMAN_INTERFACE, "Mode"))
        .await
        .context("failed to get Mode property")?;

    let val: zbus::zvariant::Value = value.into();
    let mode_str = if let zbus::zvariant::Value::Str(s) = val {
        s.to_string()
    } else {
        "light".to_string()
    };

    Ok(mode_str == "dark")
}

/// Monitor darkman D-Bus signals for dark/light mode changes.
/// Updates state and sends wallpaper commands when in StyleTracking mode.
async fn monitor_darkman_signals(
    state: Arc<StdMutex<AwwwState>>,
    wp_tx: tokio::sync::mpsc::Sender<WallpaperCommand>,
    notifier: EntityNotifier,
) -> Result<()> {
    let conn = match Connection::session().await {
        Ok(c) => c,
        Err(e) => {
            log::warn!("[awww] cannot connect to session bus for darkman monitoring: {e}");
            return Ok(());
        }
    };

    // Check if darkman is available and get initial mode
    match get_darkman_mode(&conn).await {
        Ok(dark) => {
            let mut s = lock_state(&state);
            s.style_tracking_available = true;
            s.dark_mode_active = Some(dark);
            log::info!("[awww] darkman available, dark_mode={dark}");
        }
        Err(e) => {
            log::info!("[awww] darkman not available: {e}");
            // style_tracking_available stays false (set in AwwwPlugin::new)
        }
    }
    notifier.notify();

    let config = SignalMonitorConfig::builder()
        .sender(DARKMAN_DESTINATION)
        .path(DARKMAN_PATH)
        .interface(DARKMAN_INTERFACE)
        .member("ModeChanged")
        .build()?;

    monitor_signal(conn, config, state, notifier, move |msg, awww_state| {
        let new_mode_str: String = msg.body().deserialize()?;
        let dark = new_mode_str == "dark";
        let old_dark = awww_state.dark_mode_active;
        awww_state.dark_mode_active = Some(dark);
        awww_state.style_tracking_available = true;

        log::info!("[awww] darkman mode changed: dark={dark}");

        if awww_state.mode == WallpaperMode::StyleTracking && old_dark != Some(dark) {
            let subfolder = if dark { "dark" } else { "light" };
            let dir = AwwwPlugin::expand_tilde(&awww_state.wallpaper_dir);
            let segment_dir = format!("{dir}/{subfolder}");
            if let Err(e) = wp_tx.try_send(WallpaperCommand::ApplyFromDir { dir: segment_dir }) {
                log::warn!("[awww] failed to queue style-tracking wallpaper: {e}");
            }
        }
        Ok(true)
    })
    .await
}

/// Day-segment timer that sleeps to the next boundary (NO POLLING).
/// Updates current_segment in state and sends wallpaper commands when in DayTracking mode.
async fn day_segment_timer(
    state: Arc<StdMutex<AwwwState>>,
    wp_tx: tokio::sync::mpsc::Sender<WallpaperCommand>,
    notifier: EntityNotifier,
) {
    loop {
        let now = Local::now();
        let current = DaySegment::from_time(now.hour(), now.minute());
        let secs_to_next =
            DaySegment::seconds_to_next(now.hour(), now.minute(), now.second());

        // Update current segment in state
        {
            let mut s = lock_state(&state);
            let prev_segment = s.current_segment;
            s.current_segment = Some(current);

            // Apply wallpaper if segment changed and mode is DayTracking
            if s.mode == WallpaperMode::DayTracking && prev_segment != Some(current) {
                let dir = AwwwPlugin::expand_tilde(&s.wallpaper_dir);
                let segment_dir = format!("{}/{}", dir, current.folder_name());
                if let Err(e) = wp_tx.try_send(WallpaperCommand::ApplyFromDir {
                    dir: segment_dir,
                }) {
                    log::warn!("[awww] failed to queue day-segment wallpaper: {e}");
                }
            }
        }
        notifier.notify();

        log::debug!(
            "[awww] current segment: {current:?}, sleeping {secs_to_next}s to next boundary"
        );
        tokio::time::sleep(Duration::from_secs(secs_to_next)).await;
    }
}

/// Wallpaper applicator task: receives commands and applies wallpapers.
async fn wallpaper_applicator(
    mut rx: tokio::sync::mpsc::Receiver<WallpaperCommand>,
    state: Arc<StdMutex<AwwwState>>,
    notifier: EntityNotifier,
) {
    while let Some(cmd) = rx.recv().await {
        match cmd {
            WallpaperCommand::ApplyFromDir { dir } => {
                match pick_random_wallpaper(&dir) {
                    Ok(path) => {
                        let path_str = path.to_string_lossy().to_string();
                        let (backend, transition) = {
                            let s = lock_state(&state);
                            (s.backend.clone(), s.transition.clone())
                        };
                        if let Err(e) = run_img(&backend, &path_str, None, &transition).await {
                            log::error!("[awww] failed to apply wallpaper: {e}");
                            continue;
                        }
                        // Refresh state after applying
                        match run_query(&backend).await {
                            Ok(outputs) => {
                                let mut s = lock_state(&state);
                                s.outputs = outputs;
                                s.available = true;
                            }
                            Err(e) => {
                                log::warn!("[awww] refresh after apply failed: {e}");
                            }
                        }
                        notifier.notify();
                    }
                    Err(e) => {
                        log::debug!("[awww] no wallpapers in {dir}: {e} (noop)");
                    }
                }
            }
        }
    }
    log::warn!("[awww] wallpaper applicator exited");
}

fn main() -> Result<()> {
    PluginRunner::new("awww", &[WALLPAPER_MANAGER_ENTITY_TYPE])
        .i18n(i18n(), "plugin-name", "plugin-description")
        .run(|notifier| async move {
            // Create the wallpaper command channel BEFORE creating the plugin
            let (wp_tx, wp_rx) = tokio::sync::mpsc::channel::<WallpaperCommand>(16);

            let plugin = AwwwPlugin::new(wp_tx).await?;

            // Clone all shared handles BEFORE plugin is moved into PluginRuntime
            let state_for_darkman = plugin.state.clone();
            let state_for_timer = plugin.state.clone();
            let state_for_applicator = plugin.state.clone();
            let wp_tx_darkman = plugin.wp_tx.clone();
            let wp_tx_timer = plugin.wp_tx.clone();

            // Dark-mode D-Bus monitoring (style-tracking)
            let notifier_dark = notifier.clone();
            spawn_monitored("awww-darkman", async move {
                monitor_darkman_signals(state_for_darkman, wp_tx_darkman, notifier_dark).await
            });

            // Day-segment timer (day-tracking)
            let notifier_day = notifier.clone();
            tokio::spawn(day_segment_timer(state_for_timer, wp_tx_timer, notifier_day));

            // Wallpaper applicator
            let notifier_applicator = notifier.clone();
            tokio::spawn(wallpaper_applicator(
                wp_rx,
                state_for_applicator,
                notifier_applicator,
            ));

            Ok(plugin)
        })
}
