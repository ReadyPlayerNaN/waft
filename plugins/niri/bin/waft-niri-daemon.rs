//! Niri compositor daemon -- keyboard layout, keyboard config, and display output management.
//!
//! Connects to the Niri compositor via `niri msg` CLI commands and monitors
//! the `niri msg --json event-stream` for real-time updates to keyboard layouts
//! and display output configurations.
//!
//! Entity types:
//! - `keyboard-layout` with actions: `cycle`
//! - `keyboard-layout-config` with actions: `add`, `remove`, `reorder`, `set-variant`, `rename`
//! - `display-output` with actions: `set-mode`, `toggle-vrr`, `set-scale`, `set-transform`, `set-enabled`
//!
//! Requires: Niri compositor running, `NIRI_SOCKET` environment variable set,
//! `niri` binary in PATH.

use std::sync::OnceLock;

use anyhow::{Context, Result};
use log::{debug, error, info, warn};
use std::sync::{Arc, Mutex as StdMutex};
use waft_i18n::I18n;
use waft_plugin::*;
use waft_plugin_niri::commands;
use waft_plugin_niri::config::{self, KeyboardConfigMode};
use waft_plugin_niri::display;
use waft_plugin_niri::event_stream::{self, NiriEvent};
use waft_plugin_niri::keyboard;
use waft_plugin_niri::state::NiriState;
use waft_protocol::entity::display::DisplayOutput;
use waft_protocol::entity::keyboard::{
    CONFIG_ENTITY_TYPE, ENTITY_TYPE as KEYBOARD_ENTITY_TYPE,
};

static I18N: OnceLock<I18n> = OnceLock::new();

fn i18n() -> &'static I18n {
    I18N.get_or_init(|| {
        I18n::new(&[
            ("en-US", include_str!("../locales/en-US/niri.ftl")),
            ("cs-CZ", include_str!("../locales/cs-CZ/niri.ftl")),
        ])
    })
}

struct NiriPlugin {
    state: Arc<StdMutex<NiriState>>,
}

impl NiriPlugin {
    fn lock_state(&self) -> std::sync::MutexGuard<'_, NiriState> {
        match self.state.lock() {
            Ok(g) => g,
            Err(e) => {
                warn!("[niri] mutex poisoned, recovering: {e}");
                e.into_inner()
            }
        }
    }

    async fn handle_keyboard_config_action(
        &self,
        action: &str,
        params: serde_json::Value,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        // Check if config is in an editable mode
        let (current_mode, file_path) = {
            let state = self.lock_state();
            (
                state.keyboard_config.mode.clone(),
                state.keyboard_config.file_path.clone(),
            )
        };

        if !matches!(
            current_mode,
            KeyboardConfigMode::LayoutList
                | KeyboardConfigMode::SystemDefault
                | KeyboardConfigMode::ExternalFile
        ) {
            let help = match current_mode {
                KeyboardConfigMode::Malformed => "Fix config file errors first.",
                _ => "",
            };
            return Err(format!(
                "Cannot modify layouts in {:?} mode. {}",
                current_mode, help
            )
            .into());
        }

        match action {
            "add" => {
                let layout: String = serde_json::from_value(
                    params
                        .get("layout")
                        .cloned()
                        .ok_or("Missing 'layout' parameter")?,
                )?;

                let name: String = params
                    .get("name")
                    .and_then(|v| serde_json::from_value(v.clone()).ok())
                    .unwrap_or_else(|| layout.clone());

                let (mut new_layouts, mut new_names) = {
                    let state = self.lock_state();
                    (
                        state.keyboard_config.layouts.clone(),
                        state.keyboard_config.layout_names.clone(),
                    )
                };

                if !new_layouts.contains(&layout) {
                    new_layouts.push(layout.clone());
                    new_names.push(name);
                    self.write_layouts(
                        &current_mode,
                        file_path.as_deref(),
                        &new_layouts,
                        &new_names,
                    )?;
                    info!("[niri] Added keyboard layout: {}", layout);

                    // Update state
                    {
                        let mut s = self.lock_state();
                        s.keyboard_config.layouts = new_layouts;
                        s.keyboard_config.layout_names = new_names;
                        if current_mode != KeyboardConfigMode::ExternalFile {
                            s.keyboard_config.mode = KeyboardConfigMode::LayoutList;
                        }
                    }

                    self.reload_niri_config().await;
                }
            }
            "remove" => {
                let layout: String = serde_json::from_value(
                    params
                        .get("layout")
                        .cloned()
                        .ok_or("Missing 'layout' parameter")?,
                )?;

                let (mut new_layouts, mut new_names) = {
                    let state = self.lock_state();
                    (
                        state.keyboard_config.layouts.clone(),
                        state.keyboard_config.layout_names.clone(),
                    )
                };

                if let Some(idx) = new_layouts.iter().position(|l| l == &layout) {
                    new_layouts.remove(idx);
                    if idx < new_names.len() {
                        new_names.remove(idx);
                    }
                }
                self.write_layouts(
                    &current_mode,
                    file_path.as_deref(),
                    &new_layouts,
                    &new_names,
                )?;
                info!("[niri] Removed keyboard layout: {}", layout);

                // Update state
                {
                    let mut s = self.lock_state();
                    s.keyboard_config.layouts = new_layouts;
                    s.keyboard_config.layout_names = new_names;
                }

                self.reload_niri_config().await;
            }
            "reorder" => {
                let layouts: Vec<String> = serde_json::from_value(
                    params
                        .get("layouts")
                        .cloned()
                        .ok_or("Missing 'layouts' parameter")?,
                )?;

                // Build code->name and code->variant maps from current state, then reorder
                let (new_names, new_variant) = {
                    let state = self.lock_state();
                    let code_to_name: std::collections::HashMap<&str, &str> = state
                        .keyboard_config
                        .layouts
                        .iter()
                        .zip(state.keyboard_config.layout_names.iter())
                        .map(|(c, n)| (c.as_str(), n.as_str()))
                        .collect();

                    // Build code->variant map for LayoutList mode
                    let variant_slots: Vec<String> = if let Some(ref v) = state.keyboard_config.variant {
                        v.split(',').map(|s| s.to_string()).collect()
                    } else {
                        vec![String::new(); state.keyboard_config.layouts.len()]
                    };
                    let code_to_variant: std::collections::HashMap<&str, &str> = state
                        .keyboard_config
                        .layouts
                        .iter()
                        .zip(variant_slots.iter())
                        .map(|(c, v)| (c.as_str(), v.as_str()))
                        .collect();

                    let names = layouts
                        .iter()
                        .map(|code| {
                            code_to_name
                                .get(code.as_str())
                                .unwrap_or(&"")
                                .to_string()
                        })
                        .collect::<Vec<_>>();

                    let reordered_variants: Vec<String> = layouts
                        .iter()
                        .map(|code| {
                            code_to_variant
                                .get(code.as_str())
                                .unwrap_or(&"")
                                .to_string()
                        })
                        .collect();

                    let variant = if reordered_variants.iter().all(|s| s.is_empty()) {
                        None
                    } else {
                        Some(reordered_variants.join(","))
                    };

                    (names, variant)
                };

                self.write_layouts(
                    &current_mode,
                    file_path.as_deref(),
                    &layouts,
                    &new_names,
                )?;

                // Also update variant for LayoutList mode (ExternalFile handles it in write_xkb_layouts)
                if !matches!(current_mode, KeyboardConfigMode::ExternalFile) {
                    if let Some(ref v) = new_variant {
                        config::write_keyboard_variant(v)?;
                    } else {
                        config::write_keyboard_variant("")?;
                    }
                }

                info!("[niri] Reordered keyboard layouts");

                // Update state
                {
                    let mut s = self.lock_state();
                    s.keyboard_config.layouts = layouts;
                    s.keyboard_config.layout_names = new_names;
                    s.keyboard_config.variant = new_variant;
                }

                self.reload_niri_config().await;
            }
            "set-variant" => {
                let layout: String = serde_json::from_value(
                    params
                        .get("layout")
                        .cloned()
                        .ok_or("Missing 'layout' parameter")?,
                )?;

                let variant: String = serde_json::from_value(
                    params
                        .get("variant")
                        .cloned()
                        .ok_or("Missing 'variant' parameter")?,
                )?;

                let layouts = {
                    let state = self.lock_state();
                    state.keyboard_config.layouts.clone()
                };

                let idx = layouts
                    .iter()
                    .position(|l| l == &layout)
                    .ok_or_else(|| format!("Layout '{}' not found", layout))?;

                match &current_mode {
                    KeyboardConfigMode::ExternalFile => {
                        let path = file_path
                            .as_deref()
                            .ok_or("ExternalFile mode but no file path in config")?;
                        let variant_opt = if variant.is_empty() {
                            None
                        } else {
                            Some(variant.as_str())
                        };
                        config::write_xkb_variant(path, &layout, variant_opt)?;
                    }
                    _ => {
                        // LayoutList / SystemDefault: update the comma-separated variant string
                        let current_variant = {
                            let state = self.lock_state();
                            state.keyboard_config.variant.clone()
                        };

                        let mut slots: Vec<String> = if let Some(ref v) = current_variant {
                            v.split(',').map(|s| s.to_string()).collect()
                        } else {
                            vec![String::new(); layouts.len()]
                        };

                        // Extend slots if needed
                        while slots.len() < layouts.len() {
                            slots.push(String::new());
                        }

                        slots[idx] = variant.clone();

                        // Build new variant string; set to None if all slots are empty
                        let new_variant = if slots.iter().all(|s| s.is_empty()) {
                            String::new()
                        } else {
                            slots.join(",")
                        };

                        config::write_keyboard_variant(&new_variant)?;
                    }
                }

                info!(
                    "[niri] Set variant for layout '{}' to '{}'",
                    layout,
                    if variant.is_empty() { "(none)" } else { &variant }
                );

                // Update state
                {
                    let mut s = self.lock_state();
                    // Re-derive the variant string from what we wrote
                    if current_mode == KeyboardConfigMode::ExternalFile {
                        // For XKB files, re-read the variant from file content
                        // The config reload event will update this, but set it now for responsiveness
                        let current_variant = s.keyboard_config.variant.clone();
                        let mut slots: Vec<String> = if let Some(ref v) = current_variant {
                            v.split(',').map(|s| s.to_string()).collect()
                        } else {
                            vec![String::new(); s.keyboard_config.layouts.len()]
                        };
                        while slots.len() < s.keyboard_config.layouts.len() {
                            slots.push(String::new());
                        }
                        slots[idx] = variant;
                        let new_variant = if slots.iter().all(|s| s.is_empty()) {
                            None
                        } else {
                            Some(slots.join(","))
                        };
                        s.keyboard_config.variant = new_variant;
                    } else {
                        let mut slots: Vec<String> = if let Some(ref v) = s.keyboard_config.variant {
                            v.split(',').map(|s| s.to_string()).collect()
                        } else {
                            vec![String::new(); s.keyboard_config.layouts.len()]
                        };
                        while slots.len() < s.keyboard_config.layouts.len() {
                            slots.push(String::new());
                        }
                        slots[idx] = variant;
                        s.keyboard_config.variant = if slots.iter().all(|s| s.is_empty()) {
                            None
                        } else {
                            Some(slots.join(","))
                        };
                    }
                }

                self.reload_niri_config().await;
            }
            "rename" => {
                if !matches!(current_mode, KeyboardConfigMode::ExternalFile) {
                    return Err(
                        "Rename is only supported in external-file mode".into()
                    );
                }

                let layout: String = serde_json::from_value(
                    params
                        .get("layout")
                        .cloned()
                        .ok_or("Missing 'layout' parameter")?,
                )?;

                let name: String = serde_json::from_value(
                    params
                        .get("name")
                        .cloned()
                        .ok_or("Missing 'name' parameter")?,
                )?;

                let (layouts, mut names) = {
                    let state = self.lock_state();
                    (
                        state.keyboard_config.layouts.clone(),
                        state.keyboard_config.layout_names.clone(),
                    )
                };

                if let Some(idx) = layouts.iter().position(|l| l == &layout) {
                    // Extend names vec if needed
                    while names.len() <= idx {
                        names.push(String::new());
                    }
                    names[idx] = name.clone();

                    self.write_layouts(
                        &current_mode,
                        file_path.as_deref(),
                        &layouts,
                        &names,
                    )?;
                    info!("[niri] Renamed keyboard layout '{}' to '{}'", layout, name);

                    // Update state
                    {
                        let mut s = self.lock_state();
                        s.keyboard_config.layout_names = names;
                    }

                    self.reload_niri_config().await;
                } else {
                    return Err(format!("Layout '{}' not found", layout).into());
                }
            }
            _ => {
                warn!("[niri] Unknown keyboard config action: {}", action);
            }
        }

        Ok(())
    }

    /// Write layouts to the appropriate config file based on the current mode.
    fn write_layouts(
        &self,
        mode: &KeyboardConfigMode,
        file_path: Option<&str>,
        layouts: &[String],
        names: &[String],
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        match mode {
            KeyboardConfigMode::ExternalFile => {
                let path = file_path.ok_or("ExternalFile mode but no file path in config")?;
                config::write_xkb_layouts(path, layouts, names)?;
            }
            _ => {
                config::write_keyboard_layouts(layouts.to_vec())?;
            }
        }
        Ok(())
    }

    async fn reload_niri_config(&self) {
        match commands::niri_action(&["reload-config"]).await {
            Ok(()) => {
                info!("[niri] Config reloaded successfully");
            }
            Err(e) => {
                warn!(
                    "[niri] Config reload failed (config saved but not applied): {}",
                    e
                );
            }
        }
    }
}

#[async_trait::async_trait]
impl Plugin for NiriPlugin {
    fn get_entities(&self) -> Vec<Entity> {
        let state = self.lock_state();
        let mut entities = Vec::new();

        // Keyboard layout entity
        if !state.keyboard.names.is_empty() {
            let layout = keyboard::to_entity(&state.keyboard);
            let urn = Urn::new("niri", KEYBOARD_ENTITY_TYPE, "default");
            entities.push(Entity::new(urn, KEYBOARD_ENTITY_TYPE, &layout));
        }

        // Keyboard config entity
        let config_entity = keyboard::to_config_entity(&state.keyboard_config);
        let config_urn = Urn::new("niri", CONFIG_ENTITY_TYPE, "default");
        entities.push(Entity::new(config_urn, CONFIG_ENTITY_TYPE, &config_entity));

        // Display output entities
        for (name, output_state) in &state.outputs {
            let output = display::to_entity(output_state);
            let urn = Urn::new("niri", DisplayOutput::ENTITY_TYPE, name);
            entities.push(Entity::new(urn, DisplayOutput::ENTITY_TYPE, &output));
        }

        entities
    }

    async fn handle_action(
        &self,
        urn: Urn,
        action: String,
        params: serde_json::Value,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let entity_type = urn.entity_type();

        if entity_type == KEYBOARD_ENTITY_TYPE {
            match action.as_str() {
                "cycle" => {
                    debug!("[niri] Cycling keyboard layout");
                    keyboard::switch_next().await?;
                }
                _ => {
                    debug!("[niri] Unknown keyboard action: {}", action);
                }
            }
        } else if entity_type == CONFIG_ENTITY_TYPE {
            self.handle_keyboard_config_action(&action, params).await?;
        } else if entity_type == DisplayOutput::ENTITY_TYPE {
            let output_name = urn.id().to_string();
            let output_state = {
                let state = self.lock_state();
                state.outputs.get(&output_name).cloned()
            };

            match output_state {
                Some(os) => {
                    display::handle_action(&output_name, &action, &params, &os).await?;
                }
                None => {
                    warn!("[niri] Display output not found: {}", output_name);
                }
            }
        } else {
            debug!(
                "[niri] Unknown entity type: {} (action: {})",
                entity_type, action
            );
        }

        Ok(())
    }
}

fn main() -> Result<()> {
    if waft_plugin::manifest::handle_provides_i18n(
        &[
            KEYBOARD_ENTITY_TYPE,
            CONFIG_ENTITY_TYPE,
            DisplayOutput::ENTITY_TYPE,
        ],
        i18n(),
        "plugin-name",
        "plugin-description",
    ) {
        return Ok(());
    }

    waft_plugin::init_plugin_logger("info");

    // Verify NIRI_SOCKET is set
    if std::env::var("NIRI_SOCKET").is_err() {
        error!("[niri] NIRI_SOCKET not set -- is Niri running?");
        anyhow::bail!("NIRI_SOCKET not set");
    }

    info!("Starting niri plugin...");

    let rt = tokio::runtime::Runtime::new().context("failed to create tokio runtime")?;
    rt.block_on(async {
        let state = Arc::new(StdMutex::new(NiriState::default()));

        // Load initial keyboard layouts
        match keyboard::query_layouts().await {
            Ok(response) => {
                let mut s = match state.lock() {
                    Ok(g) => g,
                    Err(e) => {
                        warn!("[niri] mutex poisoned, recovering: {e}");
                        e.into_inner()
                    }
                };
                keyboard::update_state_from_response(&mut s.keyboard, &response);
                info!(
                    "[niri] Loaded {} keyboard layouts, active index {}",
                    response.names.len(),
                    response.current_idx
                );
            }
            Err(e) => {
                warn!("[niri] Failed to query keyboard layouts: {e}");
            }
        }

        // Load keyboard config from niri config file
        match config::parse_niri_keyboard_config() {
            Ok(kb_config) => {
                let mut s = match state.lock() {
                    Ok(g) => g,
                    Err(e) => {
                        warn!("[niri] mutex poisoned, recovering: {e}");
                        e.into_inner()
                    }
                };
                info!(
                    "[niri] Loaded keyboard config: mode={:?}, {} layouts",
                    kb_config.mode,
                    kb_config.layouts.len()
                );
                s.keyboard_config = kb_config;
            }
            Err(e) => {
                warn!("[niri] Failed to parse keyboard config: {e}");
                let mut s = match state.lock() {
                    Ok(g) => g,
                    Err(e) => {
                        warn!("[niri] mutex poisoned, recovering: {e}");
                        e.into_inner()
                    }
                };
                s.keyboard_config = config::KeyboardConfig {
                    mode: KeyboardConfigMode::Malformed,
                    error_message: Some(e.to_string()),
                    ..Default::default()
                };
            }
        }

        // Load initial display outputs
        match display::query_outputs().await {
            Ok(response) => {
                let output_states = display::response_to_states(&response);
                let mut s = match state.lock() {
                    Ok(g) => g,
                    Err(e) => {
                        warn!("[niri] mutex poisoned, recovering: {e}");
                        e.into_inner()
                    }
                };
                info!("[niri] Loaded {} display outputs", output_states.len());
                for (name, os) in &output_states {
                    info!(
                        "[niri]   {} ({} {}) - {}x{}@{:.1}Hz, {} modes",
                        name,
                        os.make,
                        os.model,
                        os.modes
                            .get(os.current_mode_idx)
                            .map(|m| m.width)
                            .unwrap_or(0),
                        os.modes
                            .get(os.current_mode_idx)
                            .map(|m| m.height)
                            .unwrap_or(0),
                        os.modes
                            .get(os.current_mode_idx)
                            .map(|m| m.refresh_rate_hz())
                            .unwrap_or(0.0),
                        os.modes.len()
                    );
                }
                s.outputs = output_states;
            }
            Err(e) => {
                warn!("[niri] Failed to query display outputs: {e}");
            }
        }

        let plugin = NiriPlugin {
            state: state.clone(),
        };

        let (runtime, notifier) = PluginRuntime::new("niri", plugin);

        // Spawn event stream monitoring
        let event_rx = event_stream::spawn_event_stream();
        let event_state = state.clone();
        let event_notifier = notifier.clone();

        tokio::spawn(async move {
            while let Ok(event) = event_rx.recv_async().await {
                match event {
                    NiriEvent::KeyboardLayoutsChanged { names, current_idx } => {
                        {
                            let mut s = match event_state.lock() {
                                Ok(g) => g,
                                Err(e) => {
                                    warn!("[niri] mutex poisoned, recovering: {e}");
                                    e.into_inner()
                                }
                            };
                            s.keyboard.names = names;
                            s.keyboard.current_idx = current_idx;
                        }
                        event_notifier.notify();
                    }
                    NiriEvent::KeyboardLayoutSwitched { idx } => {
                        {
                            let mut s = match event_state.lock() {
                                Ok(g) => g,
                                Err(e) => {
                                    warn!("[niri] mutex poisoned, recovering: {e}");
                                    e.into_inner()
                                }
                            };
                            s.keyboard.current_idx = idx;
                        }
                        event_notifier.notify();
                    }
                    NiriEvent::ConfigReloaded => {
                        // Re-parse keyboard config
                        match config::parse_niri_keyboard_config() {
                            Ok(new_config) => {
                                let changed = {
                                    let mut s = match event_state.lock() {
                                        Ok(g) => g,
                                        Err(e) => {
                                            warn!("[niri] mutex poisoned, recovering: {e}");
                                            e.into_inner()
                                        }
                                    };

                                    let changed = s.keyboard_config.mode != new_config.mode
                                        || s.keyboard_config.layouts != new_config.layouts
                                        || s.keyboard_config.layout_names != new_config.layout_names
                                        || s.keyboard_config.options != new_config.options
                                        || s.keyboard_config.variant != new_config.variant;

                                    if changed {
                                        info!(
                                            "[niri] Keyboard config changed: mode={:?}, {} layouts",
                                            new_config.mode,
                                            new_config.layouts.len()
                                        );
                                        s.keyboard_config = new_config;
                                    }

                                    changed
                                };

                                if changed {
                                    event_notifier.notify();
                                }
                            }
                            Err(e) => {
                                warn!(
                                    "[niri] Failed to re-parse keyboard config after reload: {}",
                                    e
                                );
                                {
                                    let mut s = match event_state.lock() {
                                        Ok(g) => g,
                                        Err(e) => {
                                            warn!("[niri] mutex poisoned, recovering: {e}");
                                            e.into_inner()
                                        }
                                    };
                                    s.keyboard_config = config::KeyboardConfig {
                                        mode: KeyboardConfigMode::Malformed,
                                        error_message: Some(e.to_string()),
                                        ..Default::default()
                                    };
                                }
                                event_notifier.notify();
                            }
                        }

                        // Re-query outputs when config changes
                        match display::query_outputs().await {
                            Ok(response) => {
                                let output_states = display::response_to_states(&response);
                                {
                                    let mut s = match event_state.lock() {
                                        Ok(g) => g,
                                        Err(e) => {
                                            warn!("[niri] mutex poisoned, recovering: {e}");
                                            e.into_inner()
                                        }
                                    };
                                    s.outputs = output_states;
                                }
                                event_notifier.notify();
                                info!("[niri] Reloaded display outputs after config change");
                            }
                            Err(e) => {
                                warn!(
                                    "[niri] Failed to re-query outputs after config change: {e}"
                                );
                            }
                        }
                    }
                }
            }
            warn!(
                "[niri] Event stream receiver loop ended -- events will no longer be processed"
            );
        });

        runtime.run().await?;
        Ok(())
    })
}
