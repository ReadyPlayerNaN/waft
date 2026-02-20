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

use std::sync::OnceLock;

use anyhow::Result;
use log::{debug, error, info, warn};
use std::sync::{Arc, Mutex as StdMutex};
use waft_i18n::I18n;
use waft_plugin::*;

static I18N: OnceLock<I18n> = OnceLock::new();

fn i18n() -> &'static I18n {
    I18N.get_or_init(|| {
        I18n::new(&[
            ("en-US", include_str!("../locales/en-US/audio.ftl")),
            ("cs-CZ", include_str!("../locales/cs-CZ/audio.ftl")),
        ])
    })
}

use waft_plugin_audio::pactl::{self, AudioEvent, CardInfo, CardPortMap, SinkInfo, SourceInfo};

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
    lock_state(state).card_ports = card_ports.clone();

    reload_sinks(state, &card_ports).await;
    reload_sources(state, &card_ports).await;
    reload_cards(state).await;

    Ok(())
}

/// Reload sink (output) state.
async fn reload_sinks(state: &Arc<StdMutex<AudioState>>, card_ports: &CardPortMap) {
    if let Ok(default) = pactl::get_default_sink().await {
        lock_state(state).default_output = Some(default);
    }

    if let Ok(sinks) = pactl::get_sinks().await {
        let devices: Vec<pactl::AudioDevice> = sinks
            .iter()
            .map(|s| pactl::AudioDevice::from_sink(s, card_ports))
            .collect();
        let mut s = lock_state(state);
        s.output_devices = devices;
        s.sinks = sinks;
    }
}

/// Reload source (input) state.
async fn reload_sources(state: &Arc<StdMutex<AudioState>>, card_ports: &CardPortMap) {
    if let Ok(default) = pactl::get_default_source().await {
        lock_state(state).default_input = Some(default);
    }

    if let Ok(sources) = pactl::get_sources().await {
        let devices: Vec<pactl::AudioDevice> = sources
            .iter()
            .map(|s| pactl::AudioDevice::from_source(s, card_ports))
            .collect();
        let mut s = lock_state(state);
        s.input_devices = devices;
        s.sources = sources;
    }
}

/// Reload card state.
async fn reload_cards(state: &Arc<StdMutex<AudioState>>) {
    match pactl::get_cards().await {
        Ok(cards) => {
            lock_state(state).cards = cards;
        }
        Err(e) => {
            warn!("[audio] Failed to reload cards: {}", e);
        }
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

        // Output devices (audio-device entities for overview)
        for device in &state.output_devices {
            let is_default = state.default_output.as_deref() == Some(&device.id);

            let audio_device = entity::audio::AudioDevice {
                name: device.name.clone(),
                volume: device.volume,
                muted: device.muted,
                default: is_default,
                kind: entity::audio::AudioDeviceKind::Output,
                device_type: device.device_type.clone(),
                connection_type: device.connection_type.clone(),
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

            let audio_device = entity::audio::AudioDevice {
                name: device.name.clone(),
                volume: device.volume,
                muted: device.muted,
                default: is_default,
                kind: entity::audio::AudioDeviceKind::Input,
                device_type: device.device_type.clone(),
                connection_type: device.connection_type.clone(),
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
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let entity_type = urn.entity_type();

        if entity_type == entity::audio::CARD_ENTITY_TYPE {
            return self.handle_card_action(urn, action, params).await;
        }

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
                } else if is_input {
                    if let Err(e) = pactl::set_source_volume(&device_id, volume).await {
                        error!("[audio] Failed to set source volume: {}", e);
                        return Err(e.into());
                    }
                } else {
                    debug!("[audio] Unknown device for set-volume: {}", device_id);
                }
            }
            "toggle-mute" => {
                if is_output {
                    let current_muted = lock_state(&self.state)
                        .output_devices
                        .iter()
                        .find(|d| d.id == device_id)
                        .map(|d| d.muted)
                        .unwrap_or(false);
                    let new_muted = !current_muted;
                    if let Err(e) = pactl::set_sink_mute(&device_id, new_muted).await {
                        error!("[audio] Failed to toggle sink mute: {}", e);
                        return Err(e.into());
                    }
                } else if is_input {
                    let current_muted = lock_state(&self.state)
                        .input_devices
                        .iter()
                        .find(|d| d.id == device_id)
                        .map(|d| d.muted)
                        .unwrap_or(false);
                    let new_muted = !current_muted;
                    if let Err(e) = pactl::set_source_mute(&device_id, new_muted).await {
                        error!("[audio] Failed to toggle source mute: {}", e);
                        return Err(e.into());
                    }
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
                } else if is_input {
                    if let Err(e) = pactl::set_default_source(&device_id).await {
                        error!("[audio] Failed to set default source: {}", e);
                        return Err(e.into());
                    }
                    lock_state(&self.state).default_input = Some(device_id.clone());
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

impl AudioPlugin {
    /// Handle actions on audio-card entities.
    async fn handle_card_action(
        &self,
        _urn: Urn,
        action: String,
        params: serde_json::Value,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        match action.as_str() {
            "set-profile" => {
                let card_name = _urn.id().to_string();
                let profile = params
                    .get("profile")
                    .and_then(|v| v.as_str())
                    .ok_or("missing 'profile' parameter")?;
                if let Err(e) = pactl::set_card_profile(&card_name, profile).await {
                    error!("[audio] Failed to set card profile: {}", e);
                    return Err(e.into());
                }
            }
            "set-volume" => {
                let volume = params
                    .get("value")
                    .and_then(|v| v.as_f64())
                    .unwrap_or(0.0)
                    .clamp(0.0, 1.0);

                if let Some(sink) = params.get("sink").and_then(|v| v.as_str()) {
                    if let Err(e) = pactl::set_sink_volume(sink, volume).await {
                        error!("[audio] Failed to set sink volume: {}", e);
                        return Err(e.into());
                    }
                } else if let Some(source) = params.get("source").and_then(|v| v.as_str()) {
                    if let Err(e) = pactl::set_source_volume(source, volume).await {
                        error!("[audio] Failed to set source volume: {}", e);
                        return Err(e.into());
                    }
                } else {
                    debug!("[audio] set-volume on card: missing 'sink' or 'source' param");
                }
            }
            "toggle-mute" => {
                if let Some(sink_name) = params.get("sink").and_then(|v| v.as_str()) {
                    let current_muted = lock_state(&self.state)
                        .sinks
                        .iter()
                        .find(|s| s.name == sink_name)
                        .map(|s| s.muted)
                        .unwrap_or(false);
                    if let Err(e) = pactl::set_sink_mute(sink_name, !current_muted).await {
                        error!("[audio] Failed to toggle sink mute: {}", e);
                        return Err(e.into());
                    }
                } else if let Some(source_name) = params.get("source").and_then(|v| v.as_str()) {
                    let current_muted = lock_state(&self.state)
                        .sources
                        .iter()
                        .find(|s| s.name == source_name)
                        .map(|s| s.muted)
                        .unwrap_or(false);
                    if let Err(e) = pactl::set_source_mute(source_name, !current_muted).await {
                        error!("[audio] Failed to toggle source mute: {}", e);
                        return Err(e.into());
                    }
                } else {
                    debug!("[audio] toggle-mute on card: missing 'sink' or 'source' param");
                }
            }
            "set-default" => {
                if let Some(sink_name) = params.get("sink").and_then(|v| v.as_str()) {
                    if let Err(e) = pactl::set_default_sink(sink_name).await {
                        error!("[audio] Failed to set default sink: {}", e);
                        return Err(e.into());
                    }
                    lock_state(&self.state).default_output = Some(sink_name.to_string());
                } else if let Some(source_name) = params.get("source").and_then(|v| v.as_str()) {
                    if let Err(e) = pactl::set_default_source(source_name).await {
                        error!("[audio] Failed to set default source: {}", e);
                        return Err(e.into());
                    }
                    lock_state(&self.state).default_input = Some(source_name.to_string());
                } else {
                    debug!("[audio] set-default on card: missing 'sink' or 'source' param");
                }
            }
            "set-sink-port" => {
                let sink = params
                    .get("sink")
                    .and_then(|v| v.as_str())
                    .ok_or("missing 'sink' parameter")?;
                let port = params
                    .get("port")
                    .and_then(|v| v.as_str())
                    .ok_or("missing 'port' parameter")?;
                if let Err(e) = pactl::set_sink_port(sink, port).await {
                    error!("[audio] Failed to set sink port: {}", e);
                    return Err(e.into());
                }
            }
            "set-source-port" => {
                let source = params
                    .get("source")
                    .and_then(|v| v.as_str())
                    .ok_or("missing 'source' parameter")?;
                let port = params
                    .get("port")
                    .and_then(|v| v.as_str())
                    .ok_or("missing 'port' parameter")?;
                if let Err(e) = pactl::set_source_port(source, port).await {
                    error!("[audio] Failed to set source port: {}", e);
                    return Err(e.into());
                }
            }
            other => {
                debug!("[audio] Unknown card action: {}", other);
            }
        }

        Ok(())
    }
}

/// Build an AudioCard entity from card info and current audio state.
fn build_card_entity(card: &CardInfo, state: &AudioState) -> entity::audio::AudioCard {
    let icon = pactl::compute_primary_icon_sink(&card.icon_name, &None);
    let connection_icon = pactl::compute_secondary_icon(&card.icon_name, &card.bus);

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
            let sink_icon = pactl::compute_primary_icon_sink(&sink.icon_name, &sink.active_port);
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
            let source_icon =
                pactl::compute_primary_icon_source(&source.icon_name, &source.active_port);
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
        debug!("[audio] Received event: {:?}", event);

        let card_ports = pactl::get_card_port_info().await.unwrap_or_default();
        lock_state(&state).card_ports = card_ports.clone();

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
    if waft_plugin::manifest::handle_provides_i18n(
        &[entity::audio::ENTITY_TYPE, entity::audio::CARD_ENTITY_TYPE],
        i18n(),
        "plugin-name",
        "plugin-description",
    ) {
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
