//! Audio store module.
//!
//! Manages audio state for input and output devices.

use crate::store::{PluginStore, StoreOp, StoreState};

/// Represents an audio device.
#[derive(Clone, Debug, PartialEq)]
pub struct AudioDevice {
    pub id: String,
    pub name: String,
    pub icon: String,
    pub secondary_icon: Option<String>,
}

/// State for the audio plugin.
#[derive(Clone, Default)]
pub struct AudioState {
    pub available: bool,
    pub output_volume: f64,
    pub output_muted: bool,
    pub output_devices: Vec<AudioDevice>,
    pub default_output: Option<String>,
    pub input_volume: f64,
    pub input_muted: bool,
    pub input_devices: Vec<AudioDevice>,
    pub default_input: Option<String>,
}

/// Operations for the audio store.
#[derive(Clone)]
pub enum AudioOp {
    SetAvailable(bool),
    SetOutputVolume(f64),
    SetOutputMuted(bool),
    SetOutputDevices(Vec<AudioDevice>),
    SetDefaultOutput(String),
    SetInputVolume(f64),
    SetInputMuted(bool),
    SetInputDevices(Vec<AudioDevice>),
    SetDefaultInput(String),
}

impl StoreOp for AudioOp {}

impl StoreState for AudioState {
    type Config = ();
    fn configure(&mut self, _: &()) {}
}

/// Type alias for the audio store.
pub type AudioStore = PluginStore<AudioOp, AudioState>;

/// Create a new audio store instance.
pub fn create_audio_store() -> AudioStore {
    PluginStore::new(|state: &mut AudioState, op: AudioOp| match op {
        AudioOp::SetAvailable(available) => {
            if state.available != available {
                state.available = available;
                true
            } else {
                false
            }
        }
        AudioOp::SetOutputVolume(volume) => {
            let volume = volume.clamp(0.0, 1.0);
            if (state.output_volume - volume).abs() > f64::EPSILON {
                state.output_volume = volume;
                true
            } else {
                false
            }
        }
        AudioOp::SetOutputMuted(muted) => {
            if state.output_muted != muted {
                state.output_muted = muted;
                true
            } else {
                false
            }
        }
        AudioOp::SetOutputDevices(devices) => {
            if state.output_devices != devices {
                state.output_devices = devices;
                true
            } else {
                false
            }
        }
        AudioOp::SetDefaultOutput(id) => {
            if state.default_output.as_ref() != Some(&id) {
                state.default_output = Some(id);
                true
            } else {
                false
            }
        }
        AudioOp::SetInputVolume(volume) => {
            let volume = volume.clamp(0.0, 1.0);
            if (state.input_volume - volume).abs() > f64::EPSILON {
                state.input_volume = volume;
                true
            } else {
                false
            }
        }
        AudioOp::SetInputMuted(muted) => {
            if state.input_muted != muted {
                state.input_muted = muted;
                true
            } else {
                false
            }
        }
        AudioOp::SetInputDevices(devices) => {
            if state.input_devices != devices {
                state.input_devices = devices;
                true
            } else {
                false
            }
        }
        AudioOp::SetDefaultInput(id) => {
            if state.default_input.as_ref() != Some(&id) {
                state.default_input = Some(id);
                true
            } else {
                false
            }
        }
    })
}
