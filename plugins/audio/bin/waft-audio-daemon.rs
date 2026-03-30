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

use std::sync::LazyLock;

use anyhow::{Context, Result};
use log::{debug, error, info, warn};
use std::sync::{Arc, Mutex as StdMutex};
use waft_plugin::*;

static I18N: LazyLock<waft_i18n::I18n> = LazyLock::new(|| waft_i18n::I18n::new(&[
    ("en-US", include_str!("../locales/en-US/audio.ftl")),
    ("cs-CZ", include_str!("../locales/cs-CZ/audio.ftl")),
]));

fn i18n() -> &'static waft_i18n::I18n { &I18N }

use waft_plugin_audio::pactl::{self, AudioEvent, CardInfo, CardPortMap, SinkInfo, SourceInfo};
use waft_plugin_audio::virtual_device_config::{self, VirtualDeviceConfig};

/// Runtime state for a waft-managed virtual audio device.
#[derive(Clone)]
struct VirtualDeviceState {
    config: VirtualDeviceConfig,
    module_index: Option<u32>,
}

/// Shared audio state, accessible from both the plugin and the event monitor.
#[derive(Clone, Default)]
struct AudioState {
    output_devices: Vec<pactl::AudioDevice>,
    default_output: Option<String>,
    input_devices: Vec<pactl::AudioDevice>,
    default_input: Option<String>,
    available: bool,
    /// Raw sink info for building card entities.
    sinks: Vec<SinkInfo>,
    /// Raw source info for building card entities.
    sources: Vec<SourceInfo>,
    /// Card info for building card entities.
    cards: Vec<CardInfo>,
    /// Card port map for display labels.
    card_ports: CardPortMap,
    /// Waft-managed virtual devices.
    virtual_devices: Vec<VirtualDeviceState>,
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
            let mut s = lock_or_recover(&state);
            s.available = true;
        }
        info!("[audio] Audio system is available");

        // Load initial state
        if let Err(e) = reload_all(&state).await {
            warn!("[audio] Failed to load initial state: {e}");
        }

        // Reconcile virtual devices from config
        reconcile_virtual_devices(&state).await;

        Ok((
            Self {
                state: state.clone(),
            },
            state,
        ))
    }

    fn get_state(&self) -> AudioState {
        lock_or_recover(&self.state).clone()
    }
}


/// Reload all audio state from pactl into the shared state.
async fn reload_all(state: &Arc<StdMutex<AudioState>>) -> Result<()> {
    let card_ports = pactl::get_card_port_info().await.unwrap_or_default();
    lock_or_recover(state).card_ports = card_ports.clone();

    reload_sinks(state, &card_ports).await;
    reload_sources(state, &card_ports).await;
    reload_cards(state).await;

    Ok(())
}

/// Reload sink (output) state.
async fn reload_sinks(state: &Arc<StdMutex<AudioState>>, card_ports: &CardPortMap) {
    if let Ok(default) = pactl::get_default_sink().await {
        lock_or_recover(state).default_output = Some(default);
    }

    if let Ok(sinks) = pactl::get_sinks().await {
        let devices: Vec<pactl::AudioDevice> = sinks
            .iter()
            .map(|s| pactl::AudioDevice::from_sink(s, card_ports))
            .collect();
        let mut s = lock_or_recover(state);
        s.output_devices = devices;
        s.sinks = sinks;
    }
}

/// Reload source (input) state.
async fn reload_sources(state: &Arc<StdMutex<AudioState>>, card_ports: &CardPortMap) {
    if let Ok(default) = pactl::get_default_source().await {
        lock_or_recover(state).default_input = Some(default);
    }

    if let Ok(sources) = pactl::get_sources().await {
        let devices: Vec<pactl::AudioDevice> = sources
            .iter()
            .map(|s| pactl::AudioDevice::from_source(s, card_ports))
            .collect();
        let mut s = lock_or_recover(state);
        s.input_devices = devices;
        s.sources = sources;
    }
}

/// Reload card state.
async fn reload_cards(state: &Arc<StdMutex<AudioState>>) {
    match pactl::get_cards().await {
        Ok(cards) => {
            lock_or_recover(state).cards = cards;
        }
        Err(e) => {
            warn!("[audio] Failed to reload cards: {e}");
        }
    }
}

/// Reconcile virtual devices from config: load missing modules, track indices.
async fn reconcile_virtual_devices(state: &Arc<StdMutex<AudioState>>) {
    let configs = virtual_device_config::read_virtual_devices();
    if configs.is_empty() {
        return;
    }

    let loaded_modules = match pactl::list_modules_short().await {
        Ok(modules) => modules,
        Err(e) => {
            warn!("[audio] Failed to list modules for virtual device reconciliation: {e}");
            Vec::new()
        }
    };

    let mut virtual_devices = Vec::new();

    for config in configs {
        // Check if the module is already loaded by scanning arguments for sink_name
        let existing = loaded_modules.iter().find(|m| {
            let expected_module = match config.module_type.as_str() {
                "null-sink" => "module-null-sink",
                "null-source" => "module-null-source",
                _ => return false,
            };
            m.name == expected_module && m.arguments.contains(&config.sink_name)
        });

        let module_index = if let Some(m) = existing {
            debug!(
                "[audio] Virtual device '{}' already loaded as module {}",
                config.sink_name, m.index
            );
            Some(m.index)
        } else {
            // Load the missing module
            let result = match config.module_type.as_str() {
                "null-sink" => pactl::load_null_sink(&config.sink_name, &config.label).await,
                "null-source" => pactl::load_null_source(&config.sink_name, &config.label).await,
                other => {
                    warn!("[audio] Unknown module_type '{}' for virtual device '{}'", other, config.sink_name);
                    continue;
                }
            };
            match result {
                Ok(idx) => {
                    info!(
                        "[audio] Loaded virtual device '{}' as module {}",
                        config.sink_name, idx
                    );
                    Some(idx)
                }
                Err(e) => {
                    error!(
                        "[audio] Failed to load virtual device '{}': {e}",
                        config.sink_name
                    );
                    None
                }
            }
        };

        virtual_devices.push(VirtualDeviceState {
            config,
            module_index,
        });
    }

    lock_or_recover(state).virtual_devices = virtual_devices;
}

#[async_trait::async_trait]
impl Plugin for AudioPlugin {
    fn get_entities(&self) -> Vec<Entity> {
        let state = self.get_state();

        if !state.available {
            return vec![];
        }

        let mut entities = Vec::new();

        // Output devices (audio-device entities for overview)
        for device in &state.output_devices {
            let is_default = state.default_output.as_deref() == Some(&device.id);
            let is_virtual = state
                .virtual_devices
                .iter()
                .any(|vd| vd.config.sink_name == device.id);

            let audio_device = entity::audio::AudioDevice {
                name: device.name.clone(),
                volume: device.volume,
                muted: device.muted,
                default: is_default,
                kind: entity::audio::AudioDeviceKind::Output,
                device_type: device.device_type.clone(),
                connection_type: device.connection_type.clone(),
                virtual_device: is_virtual,
                sink_name: if is_virtual {
                    Some(device.id.clone())
                } else {
                    None
                },
            };
            entities.push(Entity::new(
                Urn::new("audio", entity::audio::ENTITY_TYPE, &device.id),
                entity::audio::ENTITY_TYPE,
                &audio_device,
            ));
        }

        // Input devices (audio-device entities for overview)
        for device in &state.input_devices {
            let is_default = state.default_input.as_deref() == Some(&device.id);
            let is_virtual = state
                .virtual_devices
                .iter()
                .any(|vd| vd.config.sink_name == device.id);

            let audio_device = entity::audio::AudioDevice {
                name: device.name.clone(),
                volume: device.volume,
                muted: device.muted,
                default: is_default,
                kind: entity::audio::AudioDeviceKind::Input,
                device_type: device.device_type.clone(),
                connection_type: device.connection_type.clone(),
                virtual_device: is_virtual,
                sink_name: if is_virtual {
                    Some(device.id.clone())
                } else {
                    None
                },
            };
            entities.push(Entity::new(
                Urn::new("audio", entity::audio::ENTITY_TYPE, &device.id),
                entity::audio::ENTITY_TYPE,
                &audio_device,
            ));
        }

        // Audio card entities (for settings)
        for card in &state.cards {
            let card_entity = build_card_entity(card, &state);
            entities.push(Entity::new(
                Urn::new("audio", entity::audio::CARD_ENTITY_TYPE, &card.name),
                entity::audio::CARD_ENTITY_TYPE,
                &card_entity,
            ));
        }

        entities
    }

    async fn handle_action(
        &self,
        urn: Urn,
        action: String,
        params: serde_json::Value,
    ) -> anyhow::Result<serde_json::Value> {
        let entity_type = urn.entity_type();

        if entity_type == entity::audio::CARD_ENTITY_TYPE {
            self.handle_card_action(urn, action, params).await?;
            return Ok(serde_json::Value::Null);
        }

        let device_id = urn.id().to_string();

        // Determine if the target is an output or input device
        let (is_output, is_input) = {
            let state = lock_or_recover(&self.state);
            let is_output = state.output_devices.iter().any(|d| d.id == device_id);
            let is_input = state.input_devices.iter().any(|d| d.id == device_id);
            (is_output, is_input)
        };

        match action.as_str() {
            "set-volume" => {
                let volume = params
                    .get("value")
                    .and_then(waft_plugin::serde_json::Value::as_f64)
                    .unwrap_or(0.0)
                    .clamp(0.0, 1.0);

                if is_output {
                    if let Err(e) = pactl::set_sink_volume(&device_id, volume).await {
                        error!("[audio] Failed to set sink volume: {e}");
                        return Err(e);
                    }
                } else if is_input {
                    if let Err(e) = pactl::set_source_volume(&device_id, volume).await {
                        error!("[audio] Failed to set source volume: {e}");
                        return Err(e);
                    }
                } else {
                    debug!("[audio] Unknown device for set-volume: {device_id}");
                }
            }
            "toggle-mute" => {
                if is_output {
                    let current_muted = lock_or_recover(&self.state)
                        .output_devices
                        .iter()
                        .find(|d| d.id == device_id)
                        .map(|d| d.muted)
                        .unwrap_or(false);
                    let new_muted = !current_muted;
                    if let Err(e) = pactl::set_sink_mute(&device_id, new_muted).await {
                        error!("[audio] Failed to toggle sink mute: {e}");
                        return Err(e);
                    }
                } else if is_input {
                    let current_muted = lock_or_recover(&self.state)
                        .input_devices
                        .iter()
                        .find(|d| d.id == device_id)
                        .map(|d| d.muted)
                        .unwrap_or(false);
                    let new_muted = !current_muted;
                    if let Err(e) = pactl::set_source_mute(&device_id, new_muted).await {
                        error!("[audio] Failed to toggle source mute: {e}");
                        return Err(e);
                    }
                } else {
                    debug!("[audio] Unknown device for toggle-mute: {device_id}");
                }
            }
            "set-default" => {
                if is_output {
                    if let Err(e) = pactl::set_default_sink(&device_id).await {
                        error!("[audio] Failed to set default sink: {e}");
                        return Err(e);
                    }
                    lock_or_recover(&self.state).default_output = Some(device_id.clone());
                } else if is_input {
                    if let Err(e) = pactl::set_default_source(&device_id).await {
                        error!("[audio] Failed to set default source: {e}");
                        return Err(e);
                    }
                    lock_or_recover(&self.state).default_input = Some(device_id.clone());
                } else {
                    debug!("[audio] Unknown device for set-default: {device_id}");
                }
            }
            "create-sink" => {
                self.handle_create_virtual_device("null-sink", &params).await?;
                return Ok(serde_json::Value::Null);
            }
            "create-source" => {
                self.handle_create_virtual_device("null-source", &params).await?;
                return Ok(serde_json::Value::Null);
            }
            "remove-sink" => {
                let sink_name = params
                    .get("sink_name")
                    .and_then(|v| v.as_str())
                    .context("missing 'sink_name' parameter")?;
                self.handle_remove_virtual_device(sink_name).await?;
                return Ok(serde_json::Value::Null);
            }
            "remove-source" => {
                let source_name = params
                    .get("source_name")
                    .and_then(|v| v.as_str())
                    .context("missing 'source_name' parameter")?;
                self.handle_remove_virtual_device(source_name).await?;
                return Ok(serde_json::Value::Null);
            }
            other => {
                debug!("[audio] Unknown action: {other}");
            }
        }

        Ok(serde_json::Value::Null)
    }
}

impl AudioPlugin {
    /// Create a virtual audio device (null-sink or null-source).
    async fn handle_create_virtual_device(
        &self,
        module_type: &str,
        params: &serde_json::Value,
    ) -> anyhow::Result<()> {
        let label = params
            .get("label")
            .and_then(|v| v.as_str())
            .context("missing 'label' parameter")?;

        let base_name = virtual_device_config::sanitize_sink_name(label);

        let existing_configs: Vec<VirtualDeviceConfig> = lock_or_recover(&self.state)
            .virtual_devices
            .iter()
            .map(|vd| vd.config.clone())
            .collect();

        let sink_name = virtual_device_config::ensure_unique_sink_name(&base_name, &existing_configs);

        let module_index = match module_type {
            "null-sink" => pactl::load_null_sink(&sink_name, label).await?,
            "null-source" => pactl::load_null_source(&sink_name, label).await?,
            other => anyhow::bail!("unsupported module_type: {other}"),
        };

        let config = VirtualDeviceConfig {
            module_type: module_type.to_string(),
            sink_name: sink_name.clone(),
            label: label.to_string(),
        };

        {
            let mut state = lock_or_recover(&self.state);
            state.virtual_devices.push(VirtualDeviceState {
                config: config.clone(),
                module_index: Some(module_index),
            });
        }

        // Persist to config and default.pa
        let all_configs = self.collect_virtual_configs();
        if let Err(e) = virtual_device_config::save_virtual_devices(&all_configs) {
            error!("[audio] Failed to save virtual device config: {e}");
        }
        if let Err(e) = virtual_device_config::sync_default_pa(&all_configs) {
            error!("[audio] Failed to sync default.pa: {e}");
        }

        info!("[audio] Created virtual device '{sink_name}' (module {module_index})");
        Ok(())
    }

    /// Remove a virtual audio device by sink/source name.
    async fn handle_remove_virtual_device(
        &self,
        name: &str,
    ) -> anyhow::Result<()> {
        let module_index = {
            let state = lock_or_recover(&self.state);
            state
                .virtual_devices
                .iter()
                .find(|vd| vd.config.sink_name == name)
                .and_then(|vd| vd.module_index)
        };

        // Unload the module if we have an index
        if let Some(idx) = module_index
            && let Err(e) = pactl::unload_module(idx).await
        {
            error!("[audio] Failed to unload module {idx} for '{name}': {e}");
            return Err(e);
        }

        // Remove from state
        {
            let mut state = lock_or_recover(&self.state);
            state
                .virtual_devices
                .retain(|vd| vd.config.sink_name != name);
        }

        // Persist
        let all_configs = self.collect_virtual_configs();
        if let Err(e) = virtual_device_config::save_virtual_devices(&all_configs) {
            error!("[audio] Failed to save virtual device config: {e}");
        }
        if let Err(e) = virtual_device_config::sync_default_pa(&all_configs) {
            error!("[audio] Failed to sync default.pa: {e}");
        }

        info!("[audio] Removed virtual device '{name}'");
        Ok(())
    }

    /// Collect all virtual device configs from current state.
    fn collect_virtual_configs(&self) -> Vec<VirtualDeviceConfig> {
        lock_or_recover(&self.state)
            .virtual_devices
            .iter()
            .map(|vd| vd.config.clone())
            .collect()
    }

    /// Handle actions on audio-card entities.
    async fn handle_card_action(
        &self,
        _urn: Urn,
        action: String,
        params: serde_json::Value,
    ) -> anyhow::Result<()> {
        match action.as_str() {
            "set-profile" => {
                let card_name = _urn.id().to_string();
                let profile = params
                    .get("profile")
                    .and_then(|v| v.as_str())
                    .context("missing 'profile' parameter")?;
                if let Err(e) = pactl::set_card_profile(&card_name, profile).await {
                    error!("[audio] Failed to set card profile: {e}");
                    return Err(e);
                }
            }
            "set-volume" => {
                let volume = params
                    .get("value")
                    .and_then(waft_plugin::serde_json::Value::as_f64)
                    .unwrap_or(0.0)
                    .clamp(0.0, 1.0);

                if let Some(sink) = params.get("sink").and_then(|v| v.as_str()) {
                    if let Err(e) = pactl::set_sink_volume(sink, volume).await {
                        error!("[audio] Failed to set sink volume: {e}");
                        return Err(e);
                    }
                } else if let Some(source) = params.get("source").and_then(|v| v.as_str()) {
                    if let Err(e) = pactl::set_source_volume(source, volume).await {
                        error!("[audio] Failed to set source volume: {e}");
                        return Err(e);
                    }
                } else {
                    debug!("[audio] set-volume on card: missing 'sink' or 'source' param");
                }
            }
            "toggle-mute" => {
                if let Some(sink_name) = params.get("sink").and_then(|v| v.as_str()) {
                    let current_muted = lock_or_recover(&self.state)
                        .sinks
                        .iter()
                        .find(|s| s.name == sink_name)
                        .map(|s| s.muted)
                        .unwrap_or(false);
                    if let Err(e) = pactl::set_sink_mute(sink_name, !current_muted).await {
                        error!("[audio] Failed to toggle sink mute: {e}");
                        return Err(e);
                    }
                } else if let Some(source_name) = params.get("source").and_then(|v| v.as_str()) {
                    let current_muted = lock_or_recover(&self.state)
                        .sources
                        .iter()
                        .find(|s| s.name == source_name)
                        .map(|s| s.muted)
                        .unwrap_or(false);
                    if let Err(e) = pactl::set_source_mute(source_name, !current_muted).await {
                        error!("[audio] Failed to toggle source mute: {e}");
                        return Err(e);
                    }
                } else {
                    debug!("[audio] toggle-mute on card: missing 'sink' or 'source' param");
                }
            }
            "set-default" => {
                if let Some(sink_name) = params.get("sink").and_then(|v| v.as_str()) {
                    if let Err(e) = pactl::set_default_sink(sink_name).await {
                        error!("[audio] Failed to set default sink: {e}");
                        return Err(e);
                    }
                    lock_or_recover(&self.state).default_output = Some(sink_name.to_string());
                } else if let Some(source_name) = params.get("source").and_then(|v| v.as_str()) {
                    if let Err(e) = pactl::set_default_source(source_name).await {
                        error!("[audio] Failed to set default source: {e}");
                        return Err(e);
                    }
                    lock_or_recover(&self.state).default_input = Some(source_name.to_string());
                } else {
                    debug!("[audio] set-default on card: missing 'sink' or 'source' param");
                }
            }
            "set-sink-port" => {
                let sink = params
                    .get("sink")
                    .and_then(|v| v.as_str())
                    .context("missing 'sink' parameter")?;
                let port = params
                    .get("port")
                    .and_then(|v| v.as_str())
                    .context("missing 'port' parameter")?;
                if let Err(e) = pactl::set_sink_port(sink, port).await {
                    error!("[audio] Failed to set sink port: {e}");
                    return Err(e);
                }
            }
            "set-source-port" => {
                let source = params
                    .get("source")
                    .and_then(|v| v.as_str())
                    .context("missing 'source' parameter")?;
                let port = params
                    .get("port")
                    .and_then(|v| v.as_str())
                    .context("missing 'port' parameter")?;
                if let Err(e) = pactl::set_source_port(source, port).await {
                    error!("[audio] Failed to set source port: {e}");
                    return Err(e);
                }
            }
            other => {
                debug!("[audio] Unknown card action: {other}");
            }
        }

        Ok(())
    }
}

/// Build an AudioCard entity from card info and current audio state.
fn build_card_entity(card: &CardInfo, state: &AudioState) -> entity::audio::AudioCard {
    let card_device_type = pactl::compute_device_type(
        card.form_factor.as_deref(),
        card.icon_name.as_deref(),
        None,
        false,
    );
    let card_connection_type = pactl::compute_connection_type(card.bus.as_deref(), None);

    let card_sinks: Vec<entity::audio::AudioCardSink> = state
        .sinks
        .iter()
        .filter(|sink| sink_matches_card(sink, &card.name))
        .map(|sink| {
            let label = pactl::compute_label(
                &sink.description,
                &sink.node_nick,
                &sink.device_id,
                &sink.active_port,
                &sink.icon_name,
                &sink.bus,
                &state.card_ports,
            );
            let sink_active_port_type = sink
                .ports
                .iter()
                .find(|p| Some(&p.name) == sink.active_port.as_ref())
                .and_then(|p| p.port_type.as_deref());
            let effective_port_type = match card.bus.as_deref() {
                Some("pci") => sink_active_port_type,
                _ => None,
            };
            let sink_device_type = pactl::compute_device_type(
                card.form_factor.as_deref(),
                card.icon_name.as_deref(),
                sink_active_port_type,
                false,
            );
            let sink_connection_type =
                pactl::compute_connection_type(card.bus.as_deref(), effective_port_type);
            entity::audio::AudioCardSink {
                sink_name: sink.name.clone(),
                name: label,
                volume: sink.volume_percent,
                muted: sink.muted,
                default: state.default_output.as_deref() == Some(&sink.name),
                active_port: sink.active_port.clone(),
                ports: sink
                    .ports
                    .iter()
                    .map(|p| entity::audio::AudioPort {
                        name: p.name.clone(),
                        description: p.description.clone(),
                        available: p.available,
                    })
                    .collect(),
                device_type: sink_device_type,
                connection_type: sink_connection_type,
            }
        })
        .collect();

    // Group sources that belong to this card (excluding .monitor sources, already filtered)
    let card_sources: Vec<entity::audio::AudioCardSource> = state
        .sources
        .iter()
        .filter(|source| source_matches_card(source, &card.name))
        .map(|source| {
            let label = pactl::compute_label(
                &source.description,
                &source.node_nick,
                &source.device_id,
                &source.active_port,
                &source.icon_name,
                &source.bus,
                &state.card_ports,
            );
            let source_active_port_type = source
                .ports
                .iter()
                .find(|p| Some(&p.name) == source.active_port.as_ref())
                .and_then(|p| p.port_type.as_deref());
            let effective_port_type = match card.bus.as_deref() {
                Some("pci") => source_active_port_type,
                _ => None,
            };
            let source_device_type = pactl::compute_device_type(
                card.form_factor.as_deref(),
                card.icon_name.as_deref(),
                source_active_port_type,
                true,
            );
            let source_connection_type =
                pactl::compute_connection_type(card.bus.as_deref(), effective_port_type);
            entity::audio::AudioCardSource {
                source_name: source.name.clone(),
                name: label,
                volume: source.volume_percent,
                muted: source.muted,
                default: state.default_input.as_deref() == Some(&source.name),
                active_port: source.active_port.clone(),
                ports: source
                    .ports
                    .iter()
                    .map(|p| entity::audio::AudioPort {
                        name: p.name.clone(),
                        description: p.description.clone(),
                        available: p.available,
                    })
                    .collect(),
                device_type: source_device_type,
                connection_type: source_connection_type,
            }
        })
        .collect();

    entity::audio::AudioCard {
        name: card.description.clone(),
        active_profile: card.active_profile.clone(),
        profiles: card
            .profiles
            .iter()
            .map(|p| entity::audio::AudioCardProfile {
                name: p.name.clone(),
                description: p.description.clone(),
                available: p.available,
            })
            .collect(),
        sinks: card_sinks,
        sources: card_sources,
        device_type: card_device_type,
        connection_type: card_connection_type,
    }
}

/// Match a sink to a card by comparing name patterns.
///
/// Card name `alsa_card.pci-0000_00_1f.3` maps to sinks starting with
/// `alsa_output.pci-0000_00_1f.3`. Similarly `bluez_card.XX` maps to
/// `bluez_output.XX`.
fn sink_matches_card(sink: &SinkInfo, card_name: &str) -> bool {
    if let Some(suffix) = card_name.strip_prefix("alsa_card.") {
        sink.name.starts_with(&format!("alsa_output.{suffix}"))
    } else if let Some(suffix) = card_name.strip_prefix("bluez_card.") {
        sink.name.starts_with(&format!("bluez_output.{suffix}"))
    } else if let Some(pos) = card_name.find("_card.") {
        let prefix = &card_name[..pos];
        let suffix = &card_name[pos + "_card.".len()..];
        sink.name.starts_with(&format!("{prefix}_output.{suffix}"))
    } else {
        false
    }
}

/// Match a source to a card by comparing name patterns.
fn source_matches_card(source: &SourceInfo, card_name: &str) -> bool {
    if let Some(suffix) = card_name.strip_prefix("alsa_card.") {
        source.name.starts_with(&format!("alsa_input.{suffix}"))
    } else if let Some(suffix) = card_name.strip_prefix("bluez_card.") {
        source.name.starts_with(&format!("bluez_input.{suffix}"))
    } else if let Some(pos) = card_name.find("_card.") {
        let prefix = &card_name[..pos];
        let suffix = &card_name[pos + "_card.".len()..];
        source.name.starts_with(&format!("{prefix}_input.{suffix}"))
    } else {
        false
    }
}

/// Monitor pactl events and reload state when audio devices change.
async fn monitor_events(
    mut rx: tokio::sync::mpsc::Receiver<AudioEvent>,
    state: Arc<StdMutex<AudioState>>,
    notifier: EntityNotifier,
) {
    while let Some(event) = rx.recv().await {
        debug!("[audio] Received event: {event:?}");

        let card_ports = pactl::get_card_port_info().await.unwrap_or_default();
        lock_or_recover(&state).card_ports = card_ports.clone();

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
                reload_cards(&state).await;
            }
        }

        notifier.notify();
    }

    warn!("[audio] Event monitor stopped");
}

fn main() -> Result<()> {
    PluginRunner::new("audio", &[entity::audio::ENTITY_TYPE, entity::audio::CARD_ENTITY_TYPE])
        .i18n(i18n(), "plugin-name", "plugin-description")
        .run(|notifier| async move {
            let (plugin, shared_state) = AudioPlugin::new().await?;
            let is_available = lock_or_recover(&shared_state).available;

            if is_available {
                match pactl::subscribe_events() {
                    Ok(rx) => {
                        debug!("[audio] Started event subscription");
                        spawn_monitored("audio-monitor", async move {
                            monitor_events(rx, shared_state, notifier).await;
                            Ok(())
                        });
                    }
                    Err(e) => warn!("[audio] Failed to start event subscription: {e}"),
                }
            }

            Ok(plugin)
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_device(id: &str, volume: f64, muted: bool) -> pactl::AudioDevice {
        pactl::AudioDevice {
            id: id.to_string(),
            name: format!("Device {id}"),
            device_type: "card".to_string(),
            connection_type: None,
            volume,
            muted,
        }
    }

    fn make_virtual_state(sink_name: &str, module_type: &str) -> VirtualDeviceState {
        VirtualDeviceState {
            config: VirtualDeviceConfig {
                module_type: module_type.to_string(),
                sink_name: sink_name.to_string(),
                label: format!("Virtual {sink_name}"),
            },
            module_index: Some(42),
        }
    }

    fn make_plugin(state: AudioState) -> AudioPlugin {
        AudioPlugin {
            state: Arc::new(StdMutex::new(state)),
        }
    }

    fn decode_audio_device(entity: &Entity) -> entity::audio::AudioDevice {
        serde_json::from_value(entity.data.clone()).unwrap()
    }

    #[test]
    fn virtual_output_device_gets_real_volume_and_flags() {
        let plugin = make_plugin(AudioState {
            available: true,
            output_devices: vec![make_device("waft_my_sink", 0.42, true)],
            virtual_devices: vec![make_virtual_state("waft_my_sink", "null-sink")],
            ..Default::default()
        });

        let entities = plugin.get_entities();
        let audio_entities: Vec<_> = entities
            .iter()
            .filter(|e| e.entity_type == entity::audio::ENTITY_TYPE)
            .collect();

        assert_eq!(audio_entities.len(), 1, "should emit exactly one entity for the virtual device");

        let data = decode_audio_device(audio_entities[0]);
        assert!(data.virtual_device, "virtual_device flag should be true");
        assert_eq!(data.sink_name, Some("waft_my_sink".to_string()));
        assert!((data.volume - 0.42).abs() < 0.001, "volume should be real value from pactl, not hardcoded 1.0");
        assert!(data.muted, "muted should be real value from pactl, not hardcoded false");
    }

    #[test]
    fn virtual_input_device_gets_real_volume_and_flags() {
        let plugin = make_plugin(AudioState {
            available: true,
            input_devices: vec![make_device("waft_my_source", 0.65, false)],
            virtual_devices: vec![make_virtual_state("waft_my_source", "null-source")],
            ..Default::default()
        });

        let entities = plugin.get_entities();
        let audio_entities: Vec<_> = entities
            .iter()
            .filter(|e| e.entity_type == entity::audio::ENTITY_TYPE)
            .collect();

        assert_eq!(audio_entities.len(), 1);

        let data = decode_audio_device(audio_entities[0]);
        assert!(data.virtual_device);
        assert_eq!(data.sink_name, Some("waft_my_source".to_string()));
        assert!((data.volume - 0.65).abs() < 0.001);
        assert!(!data.muted);
        assert_eq!(data.kind, entity::audio::AudioDeviceKind::Input);
    }

    #[test]
    fn regular_device_not_marked_virtual() {
        let plugin = make_plugin(AudioState {
            available: true,
            output_devices: vec![make_device("alsa_output.pci-0000", 0.8, false)],
            virtual_devices: vec![make_virtual_state("waft_unrelated", "null-sink")],
            ..Default::default()
        });

        let entities = plugin.get_entities();
        let audio_entities: Vec<_> = entities
            .iter()
            .filter(|e| e.entity_type == entity::audio::ENTITY_TYPE)
            .collect();

        assert_eq!(audio_entities.len(), 1);

        let data = decode_audio_device(audio_entities[0]);
        assert!(!data.virtual_device, "regular device should not be marked virtual");
        assert_eq!(data.sink_name, None, "regular device should have no sink_name");
    }

    #[test]
    fn no_duplicate_entities_for_virtual_device() {
        let plugin = make_plugin(AudioState {
            available: true,
            output_devices: vec![
                make_device("alsa_output.pci-0000", 0.5, false),
                make_device("waft_my_sink", 0.7, true),
            ],
            virtual_devices: vec![make_virtual_state("waft_my_sink", "null-sink")],
            ..Default::default()
        });

        let entities = plugin.get_entities();
        let audio_entities: Vec<_> = entities
            .iter()
            .filter(|e| e.entity_type == entity::audio::ENTITY_TYPE)
            .collect();

        // Should be exactly 2: one regular + one virtual (not 3 with duplicate)
        assert_eq!(audio_entities.len(), 2);

        let urns: Vec<_> = audio_entities.iter().map(|e| e.urn.to_string()).collect();
        let unique_urns: std::collections::HashSet<_> = urns.iter().collect();
        assert_eq!(urns.len(), unique_urns.len(), "all URNs should be unique (no duplicates)");
    }

    #[test]
    fn mixed_real_and_virtual_devices() {
        let plugin = make_plugin(AudioState {
            available: true,
            output_devices: vec![
                make_device("alsa_output.pci-0000", 0.5, false),
                make_device("waft_virtual_out", 0.3, true),
            ],
            input_devices: vec![
                make_device("alsa_input.pci-0000", 0.9, false),
                make_device("waft_virtual_in", 0.1, true),
            ],
            virtual_devices: vec![
                make_virtual_state("waft_virtual_out", "null-sink"),
                make_virtual_state("waft_virtual_in", "null-source"),
            ],
            ..Default::default()
        });

        let entities = plugin.get_entities();
        let audio_entities: Vec<_> = entities
            .iter()
            .filter(|e| e.entity_type == entity::audio::ENTITY_TYPE)
            .collect();

        assert_eq!(audio_entities.len(), 4, "2 outputs + 2 inputs");

        let virtual_count = audio_entities
            .iter()
            .filter(|e| decode_audio_device(e).virtual_device)
            .count();
        assert_eq!(virtual_count, 2, "exactly 2 virtual devices");

        let non_virtual_count = audio_entities
            .iter()
            .filter(|e| !decode_audio_device(e).virtual_device)
            .count();
        assert_eq!(non_virtual_count, 2, "exactly 2 non-virtual devices");
    }

    #[test]
    fn unavailable_audio_returns_empty() {
        let plugin = make_plugin(AudioState {
            available: false,
            output_devices: vec![make_device("waft_sink", 0.5, false)],
            virtual_devices: vec![make_virtual_state("waft_sink", "null-sink")],
            ..Default::default()
        });

        assert!(plugin.get_entities().is_empty());
    }
}
