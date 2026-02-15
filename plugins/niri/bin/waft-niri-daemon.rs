//! Niri compositor daemon -- keyboard layout, keyboard config, and display output management.
//!
//! Connects to the Niri compositor via `niri msg` CLI commands and monitors
//! the `niri msg --json event-stream` for real-time updates to keyboard layouts
//! and display output configurations.
//!
//! Entity types:
//! - `keyboard-layout` with actions: `cycle`
//! - `keyboard-layout-config` with actions: `add`, `remove`, `reorder`
//! - `display-output` with actions: `set-mode`, `toggle-vrr`
//!
//! Requires: Niri compositor running, `NIRI_SOCKET` environment variable set,
//! `niri` binary in PATH.

use anyhow::{Context, Result};
use log::{debug, error, info, warn};
use std::sync::{Arc, Mutex as StdMutex};
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
        let current_mode = {
            let state = self.lock_state();
            state.keyboard_config.mode.clone()
        };

        if !matches!(
            current_mode,
            KeyboardConfigMode::LayoutList | KeyboardConfigMode::SystemDefault
        ) {
            let help = match current_mode {
                KeyboardConfigMode::ExternalFile => {
                    "Remove the 'file' option from niri config to enable editing."
                }
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

                let mut new_layouts = {
                    let state = self.lock_state();
                    state.keyboard_config.layouts.clone()
                };

                if !new_layouts.contains(&layout) {
                    new_layouts.push(layout.clone());
                    config::write_keyboard_layouts(new_layouts.clone())?;
                    info!("[niri] Added keyboard layout: {}", layout);

                    // Update state
                    {
                        let mut s = self.lock_state();
                        s.keyboard_config.layouts = new_layouts;
                        s.keyboard_config.mode = KeyboardConfigMode::LayoutList;
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

                let mut new_layouts = {
                    let state = self.lock_state();
                    state.keyboard_config.layouts.clone()
                };

                new_layouts.retain(|l| l != &layout);
                config::write_keyboard_layouts(new_layouts.clone())?;
                info!("[niri] Removed keyboard layout: {}", layout);

                // Update state
                {
                    let mut s = self.lock_state();
                    s.keyboard_config.layouts = new_layouts;
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

                config::write_keyboard_layouts(layouts.clone())?;
                info!("[niri] Reordered keyboard layouts");

                // Update state
                {
                    let mut s = self.lock_state();
                    s.keyboard_config.layouts = layouts;
                }

                self.reload_niri_config().await;
            }
            _ => {
                warn!("[niri] Unknown keyboard config action: {}", action);
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
    if waft_plugin::manifest::handle_provides(&[
        KEYBOARD_ENTITY_TYPE,
        CONFIG_ENTITY_TYPE,
        DisplayOutput::ENTITY_TYPE,
    ]) {
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
