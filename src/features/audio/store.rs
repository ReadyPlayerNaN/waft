//! Audio store module.
//!
//! Manages audio state for input and output devices.

use crate::set_field;
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
        AudioOp::SetAvailable(available) => set_field!(state.available, available),
        AudioOp::SetOutputVolume(volume) => {
            let volume = volume.clamp(0.0, 1.0);
            if (state.output_volume - volume).abs() > f64::EPSILON {
                state.output_volume = volume;
                true
            } else {
                false
            }
        }
        AudioOp::SetOutputMuted(muted) => set_field!(state.output_muted, muted),
        AudioOp::SetOutputDevices(devices) => set_field!(state.output_devices, devices),
        AudioOp::SetDefaultOutput(id) => {
            let new_val = Some(id);
            set_field!(state.default_output, new_val)
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
        AudioOp::SetInputMuted(muted) => set_field!(state.input_muted, muted),
        AudioOp::SetInputDevices(devices) => set_field!(state.input_devices, devices),
        AudioOp::SetDefaultInput(id) => {
            let new_val = Some(id);
            set_field!(state.default_input, new_val)
        }
    })
}
