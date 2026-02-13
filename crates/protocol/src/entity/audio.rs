use serde::{Deserialize, Serialize};

/// Entity type identifier for audio devices.
pub const ENTITY_TYPE: &str = "audio-device";

/// An audio input or output device.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AudioDevice {
    pub name: String,
    pub icon: String,
    #[serde(default)]
    pub connection_icon: Option<String>,
    pub volume: f64,
    pub muted: bool,
    pub default: bool,
    pub kind: AudioDeviceKind,
}

/// Whether the audio device is an output (speakers/headphones) or input (microphone).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AudioDeviceKind {
    Output,
    Input,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn serde_roundtrip() {
        let device = AudioDevice {
            name: "Built-in Audio Analog Stereo".to_string(),
            icon: "audio-speakers-symbolic".to_string(),
            connection_icon: None,
            volume: 0.75,
            muted: false,
            default: true,
            kind: AudioDeviceKind::Output,
        };
        let json = serde_json::to_value(&device).unwrap();
        let decoded: AudioDevice = serde_json::from_value(json).unwrap();
        assert_eq!(device, decoded);
    }

    #[test]
    fn serde_roundtrip_input() {
        let device = AudioDevice {
            name: "USB Microphone".to_string(),
            icon: "audio-input-microphone-symbolic".to_string(),
            connection_icon: Some("bluetooth-symbolic".to_string()),
            volume: 0.5,
            muted: true,
            default: false,
            kind: AudioDeviceKind::Input,
        };
        let json = serde_json::to_value(&device).unwrap();
        let decoded: AudioDevice = serde_json::from_value(json).unwrap();
        assert_eq!(device, decoded);
    }

    #[test]
    fn backward_compat_without_connection_icon() {
        let json = serde_json::json!({
            "name": "Speakers",
            "icon": "audio-speakers-symbolic",
            "volume": 0.5,
            "muted": false,
            "default": true,
            "kind": "Output"
        });
        let device: AudioDevice = serde_json::from_value(json).unwrap();
        assert_eq!(device.connection_icon, None);
    }
}
