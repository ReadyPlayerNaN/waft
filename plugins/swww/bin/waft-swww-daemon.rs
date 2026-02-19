//! swww plugin -- wallpaper management via swww CLI.
//!
//! Provides `wallpaper-manager` entities for each display output and a
//! synthetic "all" entity for synchronized mode. Wraps `swww query` and
//! `swww img` commands.
//!
//! Configuration (in ~/.config/waft/config.toml):
//! ```toml
//! [[plugins]]
//! id = "swww"
//! wallpaper_dir = "~/.config/waft/wallpapers"
//! sync = true
//!
//! [plugins.transition]
//! transition_type = "fade"
//! fps = 60
//! angle = 0
//! duration = 1.0
//! ```

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use std::sync::{Arc, Mutex as StdMutex};

use anyhow::{Context, Result};
use serde::Deserialize;
use waft_i18n::I18n;
use waft_plugin::*;

use waft_protocol::entity::display::{
    WallpaperManager, WallpaperTransition, WALLPAPER_MANAGER_ENTITY_TYPE,
};

static I18N: OnceLock<I18n> = OnceLock::new();

fn i18n() -> &'static I18n {
    I18N.get_or_init(|| {
        I18n::new(&[
            ("en-US", include_str!("../locales/en-US/swww.ftl")),
            ("cs-CZ", include_str!("../locales/cs-CZ/swww.ftl")),
        ])
    })
}

/// Plugin configuration persisted to config.toml.
#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
struct SwwwConfig {
    #[allow(dead_code)]
    id: String,
    wallpaper_dir: String,
    sync: bool,
    transition: TransitionConfig,
}

impl Default for SwwwConfig {
    fn default() -> Self {
        Self {
            id: "swww".to_string(),
            wallpaper_dir: "~/.config/waft/wallpapers".to_string(),
            sync: true,
            transition: TransitionConfig::default(),
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

/// Shared plugin state.
struct SwwwState {
    /// Per-output current wallpaper paths (from swww query).
    outputs: HashMap<String, Option<String>>,
    /// Active transition config.
    transition: TransitionConfig,
    /// Wallpaper directory (unexpanded, as stored in config).
    wallpaper_dir: String,
    /// Sync mode.
    sync: bool,
    /// Whether swww is available.
    available: bool,
}

/// The swww plugin.
struct SwwwPlugin {
    state: Arc<StdMutex<SwwwState>>,
}

impl SwwwPlugin {
    async fn new() -> Result<Self> {
        let config: SwwwConfig =
            waft_plugin::config::load_plugin_config("swww").unwrap_or_default();

        // Attempt `swww init` best-effort
        if let Err(e) = run_swww_init().await {
            log::debug!("[swww] swww init failed (best-effort): {e}");
        }

        // Query current state
        let (available, outputs) = match run_swww_query().await {
            Ok(outputs) => (true, outputs),
            Err(e) => {
                log::warn!("[swww] swww query failed, marking unavailable: {e}");
                (false, HashMap::new())
            }
        };

        log::info!(
            "[swww] Plugin started: available={available}, outputs={}",
            outputs.len()
        );

        Ok(Self {
            state: Arc::new(StdMutex::new(SwwwState {
                outputs,
                transition: config.transition,
                wallpaper_dir: config.wallpaper_dir,
                sync: config.sync,
                available,
            })),
        })
    }

    fn lock_state(&self) -> std::sync::MutexGuard<'_, SwwwState> {
        match self.state.lock() {
            Ok(g) => g,
            Err(e) => {
                log::warn!("[swww] mutex poisoned, recovering: {e}");
                e.into_inner()
            }
        }
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

    /// Refresh outputs by re-running `swww query`.
    async fn refresh_state(&self) {
        match run_swww_query().await {
            Ok(outputs) => {
                let mut state = self.lock_state();
                state.outputs = outputs;
                state.available = true;
            }
            Err(e) => {
                log::warn!("[swww] refresh query failed: {e}");
                let mut state = self.lock_state();
                state.available = false;
            }
        }
    }

    /// Persist the current config to config.toml.
    fn persist_config(&self) {
        let state = self.lock_state();
        let config_path = match dirs::config_dir() {
            Some(d) => d.join("waft/config.toml"),
            None => {
                log::warn!("[swww] cannot determine config directory for persistence");
                return;
            }
        };

        if let Err(e) = persist_swww_config(
            &config_path,
            &state.wallpaper_dir,
            state.sync,
            &state.transition,
        ) {
            log::error!("[swww] failed to persist config: {e}");
        }
    }
}

#[async_trait::async_trait]
impl Plugin for SwwwPlugin {
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
            };
            entities.push(Entity::new(
                Urn::new("swww", WALLPAPER_MANAGER_ENTITY_TYPE, output),
                WALLPAPER_MANAGER_ENTITY_TYPE,
                &manager,
            ));
        }

        // Synthetic "all" entity
        let all_wallpaper = if state.sync {
            // When synced, use the first output's wallpaper
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
        };
        entities.push(Entity::new(
            Urn::new("swww", WALLPAPER_MANAGER_ENTITY_TYPE, "all"),
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
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let output_id = urn.id().to_string();

        match action.as_str() {
            "set-wallpaper" => {
                let path = params
                    .get("path")
                    .and_then(|v| v.as_str())
                    .ok_or("missing 'path' parameter")?;

                let (transition, sync, targets): (TransitionConfig, bool, Option<String>) = {
                    let state = self.lock_state();
                    let targets = if sync_applies(&output_id, state.sync) {
                        None
                    } else {
                        Some(output_id.clone())
                    };
                    (state.transition.clone(), state.sync, targets)
                };

                run_swww_img(path, targets.as_deref(), &transition).await?;

                // If sync mode and targeting specific output, also refresh
                if sync {
                    log::debug!("[swww] set-wallpaper in sync mode, applied to all outputs");
                }

                self.refresh_state().await;
            }

            "random" => {
                let (wallpaper_dir, transition, sync) = {
                    let state = self.lock_state();
                    (
                        Self::expand_tilde(&state.wallpaper_dir),
                        state.transition.clone(),
                        state.sync,
                    )
                };

                let path = pick_random_wallpaper(&wallpaper_dir)?;
                let path_str = path.to_string_lossy();

                let targets = if sync_applies(&output_id, sync) {
                    None
                } else {
                    Some(output_id.as_str())
                };

                run_swww_img(&path_str, targets, &transition).await?;
                self.refresh_state().await;
            }

            "update-transition" => {
                {
                    let mut state = self.lock_state();
                    if let Some(t) = params.get("transition_type").and_then(|v| v.as_str()) {
                        state.transition.transition_type = t.to_string();
                    }
                    if let Some(fps) = params.get("fps").and_then(|v| v.as_u64()) {
                        state.transition.fps = fps as u32;
                    }
                    if let Some(angle) = params.get("angle").and_then(|v| v.as_u64()) {
                        state.transition.angle = angle as u32;
                    }
                    if let Some(duration) = params.get("duration").and_then(|v| v.as_f64()) {
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
                    if let Some(sync) = params.get("sync").and_then(|v| v.as_bool()) {
                        state.sync = sync;
                    }
                }
                self.persist_config();
            }

            other => {
                log::debug!("[swww] Unknown action: {other}");
            }
        }

        Ok(())
    }

    fn can_stop(&self) -> bool {
        true
    }
}

/// Whether actions should apply to all outputs (no --outputs flag).
fn sync_applies(output_id: &str, sync: bool) -> bool {
    output_id == "all" || sync
}

/// Run `swww init` best-effort to ensure the daemon is started.
async fn run_swww_init() -> Result<()> {
    let output = tokio::process::Command::new("swww")
        .arg("init")
        .output()
        .await
        .context("failed to run swww init")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        // "already running" is not an error
        if !stderr.contains("already running") {
            anyhow::bail!("swww init failed: {stderr}");
        }
    }
    Ok(())
}

/// Run `swww query` and parse the output into a map of output -> wallpaper path.
async fn run_swww_query() -> Result<HashMap<String, Option<String>>> {
    let output = tokio::process::Command::new("swww")
        .arg("query")
        .output()
        .await
        .context("failed to run swww query")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("swww query failed: {stderr}");
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

/// Run `swww img` to set wallpaper.
async fn run_swww_img(
    path: &str,
    output: Option<&str>,
    transition: &TransitionConfig,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let mut cmd = tokio::process::Command::new("swww");
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

    let result = cmd.output().await.map_err(|e| {
        Box::new(std::io::Error::other(
            format!("failed to run swww img: {e}"),
        )) as Box<dyn std::error::Error + Send + Sync>
    })?;

    if !result.status.success() {
        let stderr = String::from_utf8_lossy(&result.stderr);
        return Err(format!("swww img failed: {stderr}").into());
    }

    Ok(())
}

/// Pick a random wallpaper from the given directory.
fn pick_random_wallpaper(dir: &str) -> Result<PathBuf, Box<dyn std::error::Error + Send + Sync>> {
    let dir_path = Path::new(dir);
    if !dir_path.exists() {
        return Err(format!("Wallpaper directory does not exist: {dir}").into());
    }

    let extensions = ["png", "jpg", "jpeg", "webp", "gif", "bmp"];
    let mut candidates = Vec::new();

    let entries = std::fs::read_dir(dir_path)
        .map_err(|e| format!("Failed to read wallpaper directory: {e}"))?;

    for entry in entries {
        let entry = match entry {
            Ok(e) => e,
            Err(e) => {
                log::debug!("[swww] skipping dir entry: {e}");
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
        return Err(format!("No wallpaper files found in {dir}").into());
    }

    let idx = fastrand::usize(..candidates.len());
    Ok(candidates.swap_remove(idx))
}

/// Persist swww config to the TOML config file using read-modify-write.
fn persist_swww_config(
    config_path: &Path,
    wallpaper_dir: &str,
    sync: bool,
    transition: &TransitionConfig,
) -> Result<()> {
    let content = if config_path.exists() {
        std::fs::read_to_string(config_path).unwrap_or_default()
    } else {
        String::new()
    };

    let mut root: toml::Table = toml::from_str(&content).unwrap_or_default();

    // Find or create the swww plugin entry
    let plugins = root
        .entry("plugins")
        .or_insert_with(|| toml::Value::Array(Vec::new()));

    if let toml::Value::Array(arr) = plugins {
        // Find existing entry
        let existing = arr.iter_mut().find(|p| {
            p.as_table()
                .and_then(|t| t.get("id"))
                .and_then(|v| v.as_str())
                == Some("swww")
        });

        let table = if let Some(entry) = existing {
            entry.as_table_mut().expect("plugin entry must be table")
        } else {
            let mut new_table = toml::Table::new();
            new_table.insert("id".to_string(), toml::Value::String("swww".to_string()));
            arr.push(toml::Value::Table(new_table));
            arr.last_mut()
                .unwrap()
                .as_table_mut()
                .expect("just inserted")
        };

        table.insert(
            "wallpaper_dir".to_string(),
            toml::Value::String(wallpaper_dir.to_string()),
        );
        table.insert("sync".to_string(), toml::Value::Boolean(sync));

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

    log::debug!("[swww] Config persisted to {}", config_path.display());
    Ok(())
}

fn main() -> Result<()> {
    // Handle `provides` CLI command before starting runtime
    if waft_plugin::manifest::handle_provides_i18n(
        &[WALLPAPER_MANAGER_ENTITY_TYPE],
        i18n(),
        "plugin-name",
        "plugin-description",
    ) {
        return Ok(());
    }

    // Initialize logging
    waft_plugin::init_plugin_logger("info");

    log::info!("Starting swww plugin...");

    let rt = tokio::runtime::Runtime::new().context("failed to create tokio runtime")?;
    rt.block_on(async {
        let plugin = SwwwPlugin::new().await?;
        let (runtime, _notifier) = PluginRuntime::new("swww", plugin);
        runtime.run().await?;
        Ok(())
    })
}
