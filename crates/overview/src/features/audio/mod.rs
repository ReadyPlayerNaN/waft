//! Audio plugin - volume control with device selection.
//!
//! Provides volume sliders for audio output (speakers) and input (microphone)
//! with expandable device menus for selecting default devices.

use anyhow::Result;
use async_trait::async_trait;
use log::{debug, error, info, warn};
use std::cell::RefCell;
use std::rc::Rc;

use gtk::prelude::*;

use crate::menu_state::MenuStore;
use crate::plugin::{Plugin, PluginId, Slot, Widget, WidgetRegistrar};

use self::control_widget::{AudioControlOutput, AudioControlProps, AudioControlWidget};
use self::dbus::{
    AudioEvent, get_card_port_info, get_default_sink, get_default_source, get_sink_volume,
    get_sinks, get_source_volume, get_sources, is_available, set_default_sink, set_default_source,
    set_sink_mute, set_sink_volume, set_source_mute, set_source_volume, subscribe_events,
};
use self::store::{AudioDevice, AudioOp, AudioStore, create_audio_store};

mod control_widget;
mod dbus;
mod device_menu;
pub mod store;

pub struct AudioPlugin {
    store: Rc<AudioStore>,
    output_control: Rc<RefCell<Option<AudioControlWidget>>>,
    input_control: Rc<RefCell<Option<AudioControlWidget>>>,
    event_channel: (flume::Sender<AudioEvent>, flume::Receiver<AudioEvent>),
}

impl AudioPlugin {
    pub fn new() -> Self {
        Self {
            store: Rc::new(create_audio_store()),
            output_control: Rc::new(RefCell::new(None)),
            input_control: Rc::new(RefCell::new(None)),
            event_channel: flume::unbounded(),
        }
    }

    async fn load_audio_state(&self) -> Result<()> {
        // Get default devices
        let default_sink = get_default_sink().await.ok();
        let default_source = get_default_source().await.ok();
        let card_ports = get_card_port_info().await.unwrap_or_default();

        // Get all sinks (outputs)
        if let Ok(sinks) = get_sinks().await {
            let devices: Vec<AudioDevice> = sinks
                .iter()
                .map(|s| AudioDevice::from_sink(s, &card_ports))
                .collect();
            self.store.emit(AudioOp::OutputDevices(devices));

            // Set default output
            if let Some(ref default) = default_sink {
                self.store.emit(AudioOp::DefaultOutput(default.clone()));

                // Get volume for default sink
                if let Ok((volume, muted)) = get_sink_volume(default).await {
                    self.store.emit(AudioOp::OutputVolume(volume));
                    self.store.emit(AudioOp::OutputMuted(muted));
                }
            }
        }

        // Get all sources (inputs)
        if let Ok(sources) = get_sources().await {
            let devices: Vec<AudioDevice> = sources
                .iter()
                .map(|s| AudioDevice::from_source(s, &card_ports))
                .collect();
            self.store.emit(AudioOp::InputDevices(devices));

            // Set default input
            if let Some(ref default) = default_source {
                self.store.emit(AudioOp::DefaultInput(default.clone()));

                // Get volume for default source
                if let Ok((volume, muted)) = get_source_volume(default).await {
                    self.store.emit(AudioOp::InputVolume(volume));
                    self.store.emit(AudioOp::InputMuted(muted));
                }
            }
        }

        Ok(())
    }
}

impl Default for AudioPlugin {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait(?Send)]
impl Plugin for AudioPlugin {
    fn id(&self) -> PluginId {
        PluginId::from_static("plugin::audio")
    }

    async fn init(&mut self) -> Result<()> {
        // Check if audio system is available
        if !is_available().await {
            warn!("[audio] PulseAudio/PipeWire not available");
            return Ok(());
        }

        self.store.emit(AudioOp::Available(true));
        info!("[audio] Audio system is available");

        // Load initial state
        if let Err(e) = self.load_audio_state().await {
            warn!("[audio] Failed to load initial state: {}", e);
        }

        // Start event subscription
        let event_tx = self.event_channel.0.clone();
        match subscribe_events(event_tx).await {
            Ok(_handle) => {
                debug!("[audio] Started event subscription");
            }
            Err(e) => {
                warn!("[audio] Failed to start event subscription: {}", e);
            }
        }

        Ok(())
    }

    async fn create_elements(
        &mut self,
        _app: &gtk::Application,
        menu_store: Rc<MenuStore>,
        registrar: Rc<dyn WidgetRegistrar>,
    ) -> Result<()> {
        let state = self.store.get_state();
        if !state.available {
            return Ok(());
        }

        // Create output control (speakers)
        let output_control = AudioControlWidget::new(
            AudioControlProps {
                icon: "audio-volume-high-symbolic".to_string(),
                volume: state.output_volume,
                muted: state.output_muted,
                devices: state.output_devices.clone(),
                default_device: state.default_output.clone(),
                input: false,
            },
            menu_store.clone(),
        );

        // Connect output control events
        let store_for_output = self.store.clone();
        output_control.connect_output(move |event| {
            let store = store_for_output.clone();
            match event {
                AudioControlOutput::VolumeChanged(volume) => {
                    glib::spawn_future_local(async move {
                        let sink = store.get_state().default_output.clone();
                        if let Some(ref sink) = sink
                            && let Err(e) = set_sink_volume(sink, volume).await {
                                error!("[audio] Failed to set sink volume: {}", e);
                            }
                    });
                }
                AudioControlOutput::ToggleMute => {
                    glib::spawn_future_local(async move {
                        let (sink, new_muted) = {
                            let state = store.get_state();
                            (state.default_output.clone(), !state.output_muted)
                        };
                        if let Some(ref sink) = sink {
                            match set_sink_mute(sink, new_muted).await {
                                Err(e) => {
                                    error!("[audio] Failed to set sink mute: {}", e);
                                }
                                _ => {
                                    store.emit(AudioOp::OutputMuted(new_muted));
                                }
                            }
                        }
                    });
                }
                AudioControlOutput::SelectDevice(device_id) => {
                    glib::spawn_future_local(async move {
                        match set_default_sink(&device_id).await {
                            Err(e) => {
                                error!("[audio] Failed to set default sink: {}", e);
                            }
                            _ => {
                                store.emit(AudioOp::DefaultOutput(device_id.clone()));

                                // Update volume for new default
                                if let Ok((volume, muted)) = get_sink_volume(&device_id).await {
                                    store.emit(AudioOp::OutputVolume(volume));
                                    store.emit(AudioOp::OutputMuted(muted));
                                }
                            }
                        }
                    });
                }
            }
        });

        *self.output_control.borrow_mut() = Some(output_control);

        // Create input control (microphone)
        let input_control = AudioControlWidget::new(
            AudioControlProps {
                icon: "audio-input-microphone-symbolic".to_string(),
                volume: state.input_volume,
                muted: state.input_muted,
                devices: state.input_devices.clone(),
                default_device: state.default_input.clone(),
                input: true,
            },
            menu_store,
        );

        // Connect input control events
        let store_for_input = self.store.clone();
        input_control.connect_output(move |event| {
            let store = store_for_input.clone();
            match event {
                AudioControlOutput::VolumeChanged(volume) => {
                    glib::spawn_future_local(async move {
                        let source = store.get_state().default_input.clone();
                        if let Some(ref source) = source
                            && let Err(e) = set_source_volume(source, volume).await {
                                error!("[audio] Failed to set source volume: {}", e);
                            }
                    });
                }
                AudioControlOutput::ToggleMute => {
                    glib::spawn_future_local(async move {
                        let (source, new_muted) = {
                            let state = store.get_state();
                            (state.default_input.clone(), !state.input_muted)
                        };
                        if let Some(ref source) = source {
                            match set_source_mute(source, new_muted).await {
                                Err(e) => {
                                    error!("[audio] Failed to set source mute: {}", e);
                                }
                                _ => {
                                    store.emit(AudioOp::InputMuted(new_muted));
                                }
                            }
                        }
                    });
                }
                AudioControlOutput::SelectDevice(device_id) => {
                    glib::spawn_future_local(async move {
                        match set_default_source(&device_id).await {
                            Err(e) => {
                                error!("[audio] Failed to set default source: {}", e);
                            }
                            _ => {
                                store.emit(AudioOp::DefaultInput(device_id.clone()));

                                // Update volume for new default
                                if let Ok((volume, muted)) = get_source_volume(&device_id).await {
                                    store.emit(AudioOp::InputVolume(volume));
                                    store.emit(AudioOp::InputMuted(muted));
                                }
                            }
                        }
                    });
                }
            }
        });

        *self.input_control.borrow_mut() = Some(input_control);

        // Register widgets
        if let Some(ref control) = *self.output_control.borrow() {
            registrar.register_widget(Rc::new(Widget {
                id: "audio:output".to_string(),
                slot: Slot::Controls,
                el: control.root.clone().upcast::<gtk::Widget>(),
                weight: 50,
            }));
        }
        if let Some(ref control) = *self.input_control.borrow() {
            registrar.register_widget(Rc::new(Widget {
                id: "audio:input".to_string(),
                slot: Slot::Controls,
                el: control.root.clone().upcast::<gtk::Widget>(),
                weight: 51,
            }));
        }

        drop(state);

        // Subscribe to store for state changes - output
        let output_control_ref = self.output_control.clone();
        let store_for_output_sub = self.store.clone();
        self.store.subscribe(move || {
            let state = store_for_output_sub.get_state();
            if let Some(ref control) = *output_control_ref.borrow() {
                control.set_volume(state.output_volume);
                control.set_muted(state.output_muted);
                control.set_devices(
                    state.output_devices.clone(),
                    state.default_output.as_deref(),
                );
            }
        });

        // Subscribe to store for state changes - input
        let input_control_ref = self.input_control.clone();
        let store_for_input_sub = self.store.clone();
        self.store.subscribe(move || {
            let state = store_for_input_sub.get_state();
            if let Some(ref control) = *input_control_ref.borrow() {
                control.set_volume(state.input_volume);
                control.set_muted(state.input_muted);
                control.set_devices(state.input_devices.clone(), state.default_input.as_deref());
            }
        });

        // Handle events from pactl subscribe
        let store_for_events = self.store.clone();
        let event_rx = self.event_channel.1.clone();

        glib::spawn_future_local(async move {
            while let Ok(event) = event_rx.recv_async().await {
                debug!("[audio] Received event: {:?}", event);

                let card_ports = get_card_port_info().await.unwrap_or_default();

                match event {
                    AudioEvent::Sink | AudioEvent::Server => {
                        // Reload sink state
                        if let Ok(default) = get_default_sink().await {
                            store_for_events.emit(AudioOp::DefaultOutput(default.clone()));

                            if let Ok((volume, muted)) = get_sink_volume(&default).await {
                                store_for_events.emit(AudioOp::OutputVolume(volume));
                                store_for_events.emit(AudioOp::OutputMuted(muted));
                            }
                        }

                        if let Ok(sinks) = get_sinks().await {
                            let devices: Vec<AudioDevice> = sinks
                                .iter()
                                .map(|s| AudioDevice::from_sink(s, &card_ports))
                                .collect();
                            store_for_events.emit(AudioOp::OutputDevices(devices));
                        }
                    }
                    AudioEvent::Source => {
                        // Reload source state
                        if let Ok(default) = get_default_source().await {
                            store_for_events.emit(AudioOp::DefaultInput(default.clone()));

                            if let Ok((volume, muted)) = get_source_volume(&default).await {
                                store_for_events.emit(AudioOp::InputVolume(volume));
                                store_for_events.emit(AudioOp::InputMuted(muted));
                            }
                        }

                        if let Ok(sources) = get_sources().await {
                            let devices: Vec<AudioDevice> = sources
                                .iter()
                                .map(|s| AudioDevice::from_source(s, &card_ports))
                                .collect();
                            store_for_events.emit(AudioOp::InputDevices(devices));
                        }
                    }
                    AudioEvent::Card => {
                        // Card change might affect available devices
                        if let Ok(sinks) = get_sinks().await {
                            let devices: Vec<AudioDevice> = sinks
                                .iter()
                                .map(|s| AudioDevice::from_sink(s, &card_ports))
                                .collect();
                            store_for_events.emit(AudioOp::OutputDevices(devices));
                        }

                        if let Ok(sources) = get_sources().await {
                            let devices: Vec<AudioDevice> = sources
                                .iter()
                                .map(|s| AudioDevice::from_source(s, &card_ports))
                                .collect();
                            store_for_events.emit(AudioOp::InputDevices(devices));
                        }
                    }
                }
            }
        });

        Ok(())
    }
}
