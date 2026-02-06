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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_state_has_zero_volumes() {
        let store = create_audio_store();
        let state = store.get_state();

        assert_eq!(state.output_volume, 0.0);
        assert_eq!(state.input_volume, 0.0);
    }

    #[test]
    fn test_default_state_is_not_available() {
        let store = create_audio_store();
        let state = store.get_state();

        assert!(!state.available);
    }

    #[test]
    fn test_default_state_is_not_muted() {
        let store = create_audio_store();
        let state = store.get_state();

        assert!(!state.output_muted);
        assert!(!state.input_muted);
    }

    #[test]
    fn test_set_available() {
        let store = create_audio_store();
        store.emit(AudioOp::SetAvailable(true));

        let state = store.get_state();
        assert!(state.available);
    }

    #[test]
    fn test_set_output_volume_normal_range() {
        let store = create_audio_store();
        store.emit(AudioOp::SetOutputVolume(0.5));

        let state = store.get_state();
        assert!((state.output_volume - 0.5).abs() < f64::EPSILON);
    }

    #[test]
    fn test_set_output_volume_clamps_above_one() {
        let store = create_audio_store();
        store.emit(AudioOp::SetOutputVolume(1.5));

        let state = store.get_state();
        assert!((state.output_volume - 1.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_set_output_volume_clamps_below_zero() {
        let store = create_audio_store();
        store.emit(AudioOp::SetOutputVolume(-0.5));

        let state = store.get_state();
        assert!((state.output_volume - 0.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_set_input_volume_clamps_above_one() {
        let store = create_audio_store();
        store.emit(AudioOp::SetInputVolume(2.0));

        let state = store.get_state();
        assert!((state.input_volume - 1.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_set_input_volume_clamps_below_zero() {
        let store = create_audio_store();
        store.emit(AudioOp::SetInputVolume(-1.0));

        let state = store.get_state();
        assert!((state.input_volume - 0.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_set_output_muted() {
        let store = create_audio_store();
        store.emit(AudioOp::SetOutputMuted(true));

        let state = store.get_state();
        assert!(state.output_muted);
    }

    #[test]
    fn test_set_input_muted() {
        let store = create_audio_store();
        store.emit(AudioOp::SetInputMuted(true));

        let state = store.get_state();
        assert!(state.input_muted);
    }

    #[test]
    fn test_set_default_output() {
        let store = create_audio_store();
        store.emit(AudioOp::SetDefaultOutput("sink-1".to_string()));

        let state = store.get_state();
        assert_eq!(state.default_output, Some("sink-1".to_string()));
    }

    #[test]
    fn test_set_default_input() {
        let store = create_audio_store();
        store.emit(AudioOp::SetDefaultInput("source-1".to_string()));

        let state = store.get_state();
        assert_eq!(state.default_input, Some("source-1".to_string()));
    }

    #[test]
    fn test_set_output_devices() {
        let store = create_audio_store();
        let devices = vec![
            AudioDevice {
                id: "sink-1".to_string(),
                name: "Speakers".to_string(),
                icon: "audio-speakers".to_string(),
                secondary_icon: None,
            },
            AudioDevice {
                id: "sink-2".to_string(),
                name: "Headphones".to_string(),
                icon: "audio-headphones".to_string(),
                secondary_icon: Some("bluetooth".to_string()),
            },
        ];
        store.emit(AudioOp::SetOutputDevices(devices.clone()));

        let state = store.get_state();
        assert_eq!(state.output_devices.len(), 2);
        assert_eq!(state.output_devices[0].id, "sink-1");
        assert_eq!(state.output_devices[1].secondary_icon, Some("bluetooth".to_string()));
    }

    #[test]
    fn test_set_input_devices() {
        let store = create_audio_store();
        let devices = vec![AudioDevice {
            id: "source-1".to_string(),
            name: "Microphone".to_string(),
            icon: "audio-input-microphone".to_string(),
            secondary_icon: None,
        }];
        store.emit(AudioOp::SetInputDevices(devices));

        let state = store.get_state();
        assert_eq!(state.input_devices.len(), 1);
        assert_eq!(state.input_devices[0].name, "Microphone");
    }

    #[test]
    fn test_volume_boundary_values() {
        let store = create_audio_store();

        // Exact boundaries
        store.emit(AudioOp::SetOutputVolume(0.0));
        assert!((store.get_state().output_volume - 0.0).abs() < f64::EPSILON);

        store.emit(AudioOp::SetOutputVolume(1.0));
        assert!((store.get_state().output_volume - 1.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_multiple_operations_in_sequence() {
        let store = create_audio_store();

        store.emit(AudioOp::SetAvailable(true));
        store.emit(AudioOp::SetOutputVolume(0.75));
        store.emit(AudioOp::SetOutputMuted(true));
        store.emit(AudioOp::SetDefaultOutput("speakers".to_string()));

        let state = store.get_state();
        assert!(state.available);
        assert!((state.output_volume - 0.75).abs() < f64::EPSILON);
        assert!(state.output_muted);
        assert_eq!(state.default_output, Some("speakers".to_string()));
    }
}
