//! Audio daemon - volume control with device selection.
//!
//! Provides volume sliders for audio output (speakers) and input (microphone)
//! with expandable device menus for selecting default devices.
//!
//! Monitors PulseAudio/PipeWire events via `pactl subscribe` and updates
//! widgets when audio state changes.

use anyhow::Result;
use log::{debug, error, info, warn};
use std::sync::{Arc, Mutex as StdMutex};
use waft_plugin_sdk::*;

use waft_plugin_audio::pactl::{self, AudioDevice, AudioEvent, CardPortMap};

/// Shared audio state, accessible from both the daemon and the event monitor.
#[derive(Clone)]
struct AudioState {
    output_volume: f64,
    output_muted: bool,
    output_devices: Vec<AudioDevice>,
    default_output: Option<String>,
    input_volume: f64,
    input_muted: bool,
    input_devices: Vec<AudioDevice>,
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

/// Audio daemon.
///
/// State is stored in `Arc<StdMutex<AudioState>>` so the background event
/// monitor can update it and then call `notifier.notify()`.
struct AudioDaemon {
    state: Arc<StdMutex<AudioState>>,
}

impl AudioDaemon {
    async fn new() -> Result<(Self, Arc<StdMutex<AudioState>>)> {
        let state = Arc::new(StdMutex::new(AudioState::default()));

        // Check if audio system is available
        if !pactl::is_available().await {
            warn!("[audio] PulseAudio/PipeWire not available");
            return Ok((Self { state: state.clone() }, state));
        }

        {
            let mut s = state.lock().unwrap();
            s.available = true;
        }
        info!("[audio] Audio system is available");

        // Load initial state
        if let Err(e) = reload_all(&state).await {
            warn!("[audio] Failed to load initial state: {}", e);
        }

        Ok((Self { state: state.clone() }, state))
    }

    fn get_state(&self) -> AudioState {
        self.state.lock().unwrap().clone()
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
            let mut s = state.lock().unwrap();
            s.default_output = Some(default.clone());
            s.output_volume = volume;
            s.output_muted = muted;
        } else {
            state.lock().unwrap().default_output = Some(default);
        }
    }

    if let Ok(sinks) = pactl::get_sinks().await {
        let devices: Vec<AudioDevice> = sinks
            .iter()
            .map(|s| AudioDevice::from_sink(s, card_ports))
            .collect();
        state.lock().unwrap().output_devices = devices;
    }
}

/// Reload source (input) state.
async fn reload_sources(state: &Arc<StdMutex<AudioState>>, card_ports: &CardPortMap) {
    if let Ok(default) = pactl::get_default_source().await {
        if let Ok((volume, muted)) = pactl::get_source_volume(&default).await {
            let mut s = state.lock().unwrap();
            s.default_input = Some(default.clone());
            s.input_volume = volume;
            s.input_muted = muted;
        } else {
            state.lock().unwrap().default_input = Some(default);
        }
    }

    if let Ok(sources) = pactl::get_sources().await {
        let devices: Vec<AudioDevice> = sources
            .iter()
            .map(|s| AudioDevice::from_source(s, card_ports))
            .collect();
        state.lock().unwrap().input_devices = devices;
    }
}

/// Compute the output volume icon based on volume level and mute state.
fn output_icon(state: &AudioState) -> String {
    if state.output_muted {
        "audio-volume-muted-symbolic".to_string()
    } else if state.output_volume < 0.01 {
        "audio-volume-muted-symbolic".to_string()
    } else if state.output_volume < 0.34 {
        "audio-volume-low-symbolic".to_string()
    } else if state.output_volume < 0.67 {
        "audio-volume-medium-symbolic".to_string()
    } else {
        "audio-volume-high-symbolic".to_string()
    }
}

/// Compute the input volume icon based on volume level and mute state.
fn input_icon(state: &AudioState) -> String {
    if state.input_muted {
        "microphone-sensitivity-muted-symbolic".to_string()
    } else if state.input_volume < 0.01 {
        "microphone-sensitivity-muted-symbolic".to_string()
    } else if state.input_volume < 0.34 {
        "microphone-sensitivity-low-symbolic".to_string()
    } else if state.input_volume < 0.67 {
        "microphone-sensitivity-medium-symbolic".to_string()
    } else {
        "microphone-sensitivity-high-symbolic".to_string()
    }
}

/// Build the device menu for output devices.
fn build_output_device_menu(state: &AudioState) -> Widget {
    let mut builder = ContainerBuilder::new(Orientation::Vertical).spacing(2);

    for device in &state.output_devices {
        let is_default = state.default_output.as_deref() == Some(&device.id);
        let row = MenuRowBuilder::new(&device.name)
            .icon(&device.icon)
            .on_click(format!("select_output:{}", device.id));

        let row = if is_default {
            row.trailing(Widget::Checkmark { visible: true })
        } else {
            row
        };

        builder = builder.child(row.build());
    }

    builder.build()
}

/// Build the device menu for input devices.
fn build_input_device_menu(state: &AudioState) -> Widget {
    let mut builder = ContainerBuilder::new(Orientation::Vertical).spacing(2);

    for device in &state.input_devices {
        let is_default = state.default_input.as_deref() == Some(&device.id);
        let row = MenuRowBuilder::new(&device.name)
            .icon(&device.icon)
            .on_click(format!("select_input:{}", device.id));

        let row = if is_default {
            row.trailing(Widget::Checkmark { visible: true })
        } else {
            row
        };

        builder = builder.child(row.build());
    }

    builder.build()
}

#[async_trait::async_trait]
impl PluginDaemon for AudioDaemon {
    fn get_widgets(&self) -> Vec<NamedWidget> {
        let state = self.get_state();

        if !state.available {
            return vec![];
        }

        let mut widgets = Vec::new();

        // Output (speakers) slider
        let output_slider = {
            let mut builder = SliderBuilder::new(state.output_volume)
                .icon(output_icon(&state))
                .muted(state.output_muted)
                .on_value_change("set_output_volume")
                .on_icon_click("toggle_output_mute");

            if state.output_devices.len() > 1 {
                builder = builder.expanded_content(build_output_device_menu(&state));
            }

            builder.build()
        };

        widgets.push(NamedWidget {
            id: "audio:output".to_string(),
            weight: 50,
            widget: output_slider,
        });

        // Input (microphone) slider
        let input_slider = {
            let mut builder = SliderBuilder::new(state.input_volume)
                .icon(input_icon(&state))
                .muted(state.input_muted)
                .on_value_change("set_input_volume")
                .on_icon_click("toggle_input_mute");

            if state.input_devices.len() > 1 {
                builder = builder.expanded_content(build_input_device_menu(&state));
            }

            builder.build()
        };

        widgets.push(NamedWidget {
            id: "audio:input".to_string(),
            weight: 51,
            widget: input_slider,
        });

        widgets
    }

    async fn handle_action(
        &mut self,
        _widget_id: String,
        action: Action,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        match action.id.as_str() {
            "set_output_volume" => {
                if let ActionParams::Value(value) = action.params {
                    let volume = value.clamp(0.0, 1.0);
                    let sink = self.state.lock().unwrap().default_output.clone();
                    if let Some(ref sink) = sink {
                        if let Err(e) = pactl::set_sink_volume(sink, volume).await {
                            error!("[audio] Failed to set sink volume: {}", e);
                            return Err(e.into());
                        }
                        self.state.lock().unwrap().output_volume = volume;
                    }
                }
            }
            "toggle_output_mute" => {
                let (sink, new_muted) = {
                    let s = self.state.lock().unwrap();
                    (s.default_output.clone(), !s.output_muted)
                };
                if let Some(ref sink) = sink {
                    if let Err(e) = pactl::set_sink_mute(sink, new_muted).await {
                        error!("[audio] Failed to toggle sink mute: {}", e);
                        return Err(e.into());
                    }
                    self.state.lock().unwrap().output_muted = new_muted;
                }
            }
            "set_input_volume" => {
                if let ActionParams::Value(value) = action.params {
                    let volume = value.clamp(0.0, 1.0);
                    let source = self.state.lock().unwrap().default_input.clone();
                    if let Some(ref source) = source {
                        if let Err(e) = pactl::set_source_volume(source, volume).await {
                            error!("[audio] Failed to set source volume: {}", e);
                            return Err(e.into());
                        }
                        self.state.lock().unwrap().input_volume = volume;
                    }
                }
            }
            "toggle_input_mute" => {
                let (source, new_muted) = {
                    let s = self.state.lock().unwrap();
                    (s.default_input.clone(), !s.input_muted)
                };
                if let Some(ref source) = source {
                    if let Err(e) = pactl::set_source_mute(source, new_muted).await {
                        error!("[audio] Failed to toggle source mute: {}", e);
                        return Err(e.into());
                    }
                    self.state.lock().unwrap().input_muted = new_muted;
                }
            }
            other => {
                if let Some(device_id) = other.strip_prefix("select_output:") {
                    if let Err(e) = pactl::set_default_sink(device_id).await {
                        error!("[audio] Failed to set default sink: {}", e);
                        return Err(e.into());
                    }
                    self.state.lock().unwrap().default_output = Some(device_id.to_string());

                    // Reload volume for new default
                    if let Ok((volume, muted)) = pactl::get_sink_volume(device_id).await {
                        let mut s = self.state.lock().unwrap();
                        s.output_volume = volume;
                        s.output_muted = muted;
                    }
                } else if let Some(device_id) = other.strip_prefix("select_input:") {
                    if let Err(e) = pactl::set_default_source(device_id).await {
                        error!("[audio] Failed to set default source: {}", e);
                        return Err(e.into());
                    }
                    self.state.lock().unwrap().default_input = Some(device_id.to_string());

                    // Reload volume for new default
                    if let Ok((volume, muted)) = pactl::get_source_volume(device_id).await {
                        let mut s = self.state.lock().unwrap();
                        s.input_volume = volume;
                        s.input_muted = muted;
                    }
                } else {
                    debug!("[audio] Unknown action: {}", other);
                }
            }
        }

        Ok(())
    }
}

/// Monitor pactl events and reload state when audio devices change.
async fn monitor_events(
    mut rx: tokio::sync::mpsc::Receiver<AudioEvent>,
    state: Arc<StdMutex<AudioState>>,
    notifier: WidgetNotifier,
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

#[tokio::main]
async fn main() -> Result<()> {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

    info!("Starting audio daemon...");

    let (daemon, shared_state) = AudioDaemon::new().await?;
    let is_available = shared_state.lock().unwrap().available;

    // Subscribe to pactl events before moving the daemon into the server
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

    let (server, notifier) = PluginServer::new("audio-daemon", daemon);

    // Start event monitoring if we have a subscription
    if let Some(rx) = event_rx {
        tokio::spawn(async move {
            monitor_events(rx, shared_state, notifier).await;
        });
    }

    server.run().await?;

    Ok(())
}
