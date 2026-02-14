//! Audio daemon -- volume control with device selection.
//!
//! Exposes one entity per audio device (sinks and sources) with volume, mute,
//! and default status. Monitors PulseAudio/PipeWire events via `pactl subscribe`
//! and updates entities when audio state changes.
//!
//! Actions per device:
//! - `set-volume` with params `{ "value": 0.75 }`
//! - `toggle-mute`
//! - `set-default`
//!
//! Configuration (in ~/.config/waft/config.toml):
//! ```toml
//! [[plugins]]
//! id = "audio"
//! ```

use anyhow::Result;
use log::{debug, error, info, warn};
use std::sync::{Arc, Mutex as StdMutex};
use waft_plugin::*;

use waft_plugin_audio::pactl::{self, AudioEvent, CardPortMap};

/// Shared audio state, accessible from both the plugin and the event monitor.
#[derive(Clone)]
struct AudioState {
    output_volume: f64,
    output_muted: bool,
    output_devices: Vec<pactl::AudioDevice>,
    default_output: Option<String>,
    input_volume: f64,
    input_muted: bool,
    input_devices: Vec<pactl::AudioDevice>,
    default_input: Option<String>,
    available: bool,
}

impl Default for AudioState {
    fn default() -> Self {
        Self {
            output_volume: 0.0,
            output_muted: false,
            output_devices: Vec::new(),
            default_output: None,
            input_volume: 0.0,
            input_muted: false,
            input_devices: Vec::new(),
            default_input: None,
            available: false,
        }
    }
}

/// Audio plugin.
///
/// State is stored in `Arc<StdMutex<AudioState>>` so the background event
/// monitor can update it and then call `notifier.notify()`.
struct AudioPlugin {
    state: Arc<StdMutex<AudioState>>,
}

impl AudioPlugin {
    async fn new() -> Result<(Self, Arc<StdMutex<AudioState>>)> {
        let state = Arc::new(StdMutex::new(AudioState::default()));

        // Check if audio system is available
        if !pactl::is_available().await {
            warn!("[audio] PulseAudio/PipeWire not available");
            return Ok((
                Self {
                    state: state.clone(),
                },
                state,
            ));
        }

        {
            let mut s = lock_state(&state);
            s.available = true;
        }
        info!("[audio] Audio system is available");

        // Load initial state
        if let Err(e) = reload_all(&state).await {
            warn!("[audio] Failed to load initial state: {}", e);
        }

        Ok((
            Self {
                state: state.clone(),
            },
            state,
        ))
    }

    fn get_state(&self) -> AudioState {
        lock_state(&self.state).clone()
    }
}

fn lock_state(state: &Arc<StdMutex<AudioState>>) -> std::sync::MutexGuard<'_, AudioState> {
    match state.lock() {
        Ok(g) => g,
        Err(e) => {
            warn!("[audio] mutex poisoned, recovering: {e}");
            e.into_inner()
        }
    }
}

/// Reload all audio state from pactl into the shared state.
async fn reload_all(state: &Arc<StdMutex<AudioState>>) -> Result<()> {
    let card_ports = pactl::get_card_port_info().await.unwrap_or_default();

    reload_sinks(state, &card_ports).await;
    reload_sources(state, &card_ports).await;

    Ok(())
}

/// Reload sink (output) state.
async fn reload_sinks(state: &Arc<StdMutex<AudioState>>, card_ports: &CardPortMap) {
    if let Ok(default) = pactl::get_default_sink().await {
        if let Ok((volume, muted)) = pactl::get_sink_volume(&default).await {
            let mut s = lock_state(state);
            s.default_output = Some(default.clone());
            s.output_volume = volume;
            s.output_muted = muted;
        } else {
            lock_state(state).default_output = Some(default);
        }
    }

    if let Ok(sinks) = pactl::get_sinks().await {
        let devices: Vec<pactl::AudioDevice> = sinks
            .iter()
            .map(|s| pactl::AudioDevice::from_sink(s, card_ports))
            .collect();
        lock_state(state).output_devices = devices;
    }
}

/// Reload source (input) state.
async fn reload_sources(state: &Arc<StdMutex<AudioState>>, card_ports: &CardPortMap) {
    if let Ok(default) = pactl::get_default_source().await {
        if let Ok((volume, muted)) = pactl::get_source_volume(&default).await {
            let mut s = lock_state(state);
            s.default_input = Some(default.clone());
            s.input_volume = volume;
            s.input_muted = muted;
        } else {
            lock_state(state).default_input = Some(default);
        }
    }

    if let Ok(sources) = pactl::get_sources().await {
        let devices: Vec<pactl::AudioDevice> = sources
            .iter()
            .map(|s| pactl::AudioDevice::from_source(s, card_ports))
            .collect();
        lock_state(state).input_devices = devices;
    }
}

#[async_trait::async_trait]
impl Plugin for AudioPlugin {
    fn get_entities(&self) -> Vec<Entity> {
        let state = self.get_state();

        if !state.available {
            return vec![];
        }

        let mut entities = Vec::new();

        // Output devices
        for device in &state.output_devices {
            let is_default = state.default_output.as_deref() == Some(&device.id);
            let (volume, muted) = if is_default {
                (state.output_volume, state.output_muted)
            } else {
                // Non-default devices don't have separate volume tracking;
                // use the default volume as a reasonable fallback
                (state.output_volume, state.output_muted)
            };

            let audio_device = entity::audio::AudioDevice {
                name: device.name.clone(),
                icon: device.icon.clone(),
                connection_icon: device.secondary_icon.clone(),
                volume,
                muted,
                default: is_default,
                kind: entity::audio::AudioDeviceKind::Output,
            };
            entities.push(Entity::new(
                Urn::new("audio", entity::audio::ENTITY_TYPE, &device.id),
                entity::audio::ENTITY_TYPE,
                &audio_device,
            ));
        }

        // Input devices
        for device in &state.input_devices {
            let is_default = state.default_input.as_deref() == Some(&device.id);
            let (volume, muted) = (state.input_volume, state.input_muted);

            let audio_device = entity::audio::AudioDevice {
                name: device.name.clone(),
                icon: device.icon.clone(),
                connection_icon: device.secondary_icon.clone(),
                volume,
                muted,
                default: is_default,
                kind: entity::audio::AudioDeviceKind::Input,
            };
            entities.push(Entity::new(
                Urn::new("audio", entity::audio::ENTITY_TYPE, &device.id),
                entity::audio::ENTITY_TYPE,
                &audio_device,
            ));
        }

        entities
    }

    async fn handle_action(
        &self,
        urn: Urn,
        action: String,
        params: serde_json::Value,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let device_id = urn.id().to_string();

        // Determine if the target is an output or input device
        let (is_output, is_input) = {
            let state = lock_state(&self.state);
            let is_output = state.output_devices.iter().any(|d| d.id == device_id);
            let is_input = state.input_devices.iter().any(|d| d.id == device_id);
            (is_output, is_input)
        };

        match action.as_str() {
            "set-volume" => {
                let volume = params
                    .get("value")
                    .and_then(|v| v.as_f64())
                    .unwrap_or(0.0)
                    .clamp(0.0, 1.0);

                if is_output {
                    if let Err(e) = pactl::set_sink_volume(&device_id, volume).await {
                        error!("[audio] Failed to set sink volume: {}", e);
                        return Err(e.into());
                    }
                    lock_state(&self.state).output_volume = volume;
                } else if is_input {
                    if let Err(e) = pactl::set_source_volume(&device_id, volume).await {
                        error!("[audio] Failed to set source volume: {}", e);
                        return Err(e.into());
                    }
                    lock_state(&self.state).input_volume = volume;
                } else {
                    debug!("[audio] Unknown device for set-volume: {}", device_id);
                }
            }
            "toggle-mute" => {
                if is_output {
                    let new_muted = !lock_state(&self.state).output_muted;
                    if let Err(e) = pactl::set_sink_mute(&device_id, new_muted).await {
                        error!("[audio] Failed to toggle sink mute: {}", e);
                        return Err(e.into());
                    }
                    lock_state(&self.state).output_muted = new_muted;
                } else if is_input {
                    let new_muted = !lock_state(&self.state).input_muted;
                    if let Err(e) = pactl::set_source_mute(&device_id, new_muted).await {
                        error!("[audio] Failed to toggle source mute: {}", e);
                        return Err(e.into());
                    }
                    lock_state(&self.state).input_muted = new_muted;
                } else {
                    debug!("[audio] Unknown device for toggle-mute: {}", device_id);
                }
            }
            "set-default" => {
                if is_output {
                    if let Err(e) = pactl::set_default_sink(&device_id).await {
                        error!("[audio] Failed to set default sink: {}", e);
                        return Err(e.into());
                    }
                    lock_state(&self.state).default_output = Some(device_id.clone());

                    // Reload volume for new default
                    if let Ok((volume, muted)) = pactl::get_sink_volume(&device_id).await {
                        let mut s = lock_state(&self.state);
                        s.output_volume = volume;
                        s.output_muted = muted;
                    }
                } else if is_input {
                    if let Err(e) = pactl::set_default_source(&device_id).await {
                        error!("[audio] Failed to set default source: {}", e);
                        return Err(e.into());
                    }
                    lock_state(&self.state).default_input = Some(device_id.clone());

                    // Reload volume for new default
                    if let Ok((volume, muted)) = pactl::get_source_volume(&device_id).await {
                        let mut s = lock_state(&self.state);
                        s.input_volume = volume;
                        s.input_muted = muted;
                    }
                } else {
                    debug!("[audio] Unknown device for set-default: {}", device_id);
                }
            }
            other => {
                debug!("[audio] Unknown action: {}", other);
            }
        }

        Ok(())
    }
}

/// Monitor pactl events and reload state when audio devices change.
async fn monitor_events(
    mut rx: tokio::sync::mpsc::Receiver<AudioEvent>,
    state: Arc<StdMutex<AudioState>>,
    notifier: EntityNotifier,
) {
    while let Some(event) = rx.recv().await {
        debug!("[audio] Received event: {:?}", event);

        let card_ports = pactl::get_card_port_info().await.unwrap_or_default();

        match event {
            AudioEvent::Sink | AudioEvent::Server => {
                reload_sinks(&state, &card_ports).await;
            }
            AudioEvent::Source => {
                reload_sources(&state, &card_ports).await;
            }
            AudioEvent::Card => {
                reload_sinks(&state, &card_ports).await;
                reload_sources(&state, &card_ports).await;
            }
        }

        notifier.notify();
    }

    warn!("[audio] Event monitor stopped");
}

fn main() -> Result<()> {
    if waft_plugin::manifest::handle_provides(&[entity::audio::ENTITY_TYPE]) {
        return Ok(());
    }

    waft_plugin::init_plugin_logger("info");

    info!("Starting audio plugin...");

    let rt = tokio::runtime::Runtime::new()?;
    rt.block_on(async {
        let (plugin, shared_state) = AudioPlugin::new().await?;
        let is_available = lock_state(&shared_state).available;

        // Subscribe to pactl events before moving the plugin into the runtime
        let event_rx = if is_available {
            match pactl::subscribe_events() {
                Ok(rx) => {
                    debug!("[audio] Started event subscription");
                    Some(rx)
                }
                Err(e) => {
                    warn!("[audio] Failed to start event subscription: {}", e);
                    None
                }
            }
        } else {
            None
        };

        let (runtime, notifier) = PluginRuntime::new("audio", plugin);

        // Start event monitoring if we have a subscription
        if let Some(rx) = event_rx {
            tokio::spawn(async move {
                monitor_events(rx, shared_state, notifier).await;
            });
        }

        runtime.run().await?;

        Ok(())
    })
}
