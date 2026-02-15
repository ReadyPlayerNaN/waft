//! Niri compositor daemon -- keyboard layout and display output management.
//!
//! Connects to the Niri compositor via `niri msg` CLI commands and monitors
//! the event stream for real-time updates to keyboard layouts and display
//! output configurations.
//!
//! Entity types:
//! - `keyboard-layout` with actions: `cycle`
//! - `display-output` with actions: `set-mode`, `toggle-vrr`
//!
//! Requires: Niri compositor running, `NIRI_SOCKET` environment variable set,
//! `niri` binary in PATH.

use anyhow::{Context, Result};
use log::{debug, error, info, warn};
use std::sync::{Arc, Mutex as StdMutex};
use waft_plugin::*;
use waft_plugin_niri::display;
use waft_plugin_niri::event_stream::{self, NiriEvent};
use waft_plugin_niri::keyboard;
use waft_plugin_niri::state::NiriState;
use waft_protocol::entity::display::DisplayOutput;
use waft_protocol::entity::keyboard::ENTITY_TYPE as KEYBOARD_ENTITY_TYPE;

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
    if waft_plugin::manifest::handle_provides(&[KEYBOARD_ENTITY_TYPE, DisplayOutput::ENTITY_TYPE]) {
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
                                warn!("[niri] Failed to re-query outputs after config change: {e}");
                            }
                        }
                    }
                }
            }
            warn!("[niri] Event stream receiver loop ended -- events will no longer be processed");
        });

        runtime.run().await?;
        Ok(())
    })
}
