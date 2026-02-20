use serde::{Deserialize, Serialize};

/// Entity type identifier for audio devices.
pub const ENTITY_TYPE: &str = "audio-device";

/// Entity type identifier for audio cards (physical devices grouping sinks/sources).
pub const CARD_ENTITY_TYPE: &str = "audio-card";

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

/// A physical audio card grouping output sinks and input sources.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AudioCard {
    /// Display name of the physical device (e.g. "Built-in Audio", "WH-1000XM4").
    pub name: String,
    /// Primary icon for the card.
    pub icon: String,
    /// Connection type icon (e.g. bluetooth, USB).
    #[serde(default)]
    pub connection_icon: Option<String>,
    /// Currently active profile name.
    pub active_profile: String,
    /// Available profiles.
    pub profiles: Vec<AudioCardProfile>,
    /// Output sinks belonging to this card.
    pub sinks: Vec<AudioCardSink>,
    /// Input sources belonging to this card (excludes monitor sources).
    pub sources: Vec<AudioCardSource>,
}

/// A profile available on an audio card.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AudioCardProfile {
    /// Internal profile name (e.g. "output:analog-stereo+input:analog-stereo").
    pub name: String,
    /// Human-readable description (e.g. "Analog Stereo Duplex").
    pub description: String,
    /// Whether this profile is available.
    pub available: bool,
}

/// An output sink belonging to an audio card.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AudioCardSink {
    /// PulseAudio sink name (for actions).
    pub sink_name: String,
    /// Display name.
    pub name: String,
    /// Icon.
    pub icon: String,
    /// Volume 0.0-1.0.
    pub volume: f64,
    /// Muted state.
    pub muted: bool,
    /// Whether this is the default output.
    pub default: bool,
    /// Active port name (if any).
    #[serde(default)]
    pub active_port: Option<String>,
    /// Available ports on this sink.
    pub ports: Vec<AudioPort>,
}

/// An input source belonging to an audio card.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AudioCardSource {
    /// PulseAudio source name (for actions).
    pub source_name: String,
    /// Display name.
    pub name: String,
    /// Icon.
    pub icon: String,
    /// Volume 0.0-1.0.
    pub volume: f64,
    /// Muted state.
    pub muted: bool,
    /// Whether this is the default input.
    pub default: bool,
    /// Active port name (if any).
    #[serde(default)]
    pub active_port: Option<String>,
    /// Available ports on this source.
    pub ports: Vec<AudioPort>,
}

/// A port on an audio sink or source.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AudioPort {
    /// Internal port name (e.g. "analog-output-speaker").
    pub name: String,
    /// Human-readable description (e.g. "Speaker").
    pub description: String,
    /// Whether the port is available (plugged in).
    pub available: bool,
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

    #[test]
    fn audio_card_serde_roundtrip() {
        let card = AudioCard {
            name: "Built-in Audio".to_string(),
            icon: "audio-card-symbolic".to_string(),
            connection_icon: None,
            active_profile: "output:analog-stereo+input:analog-stereo".to_string(),
            profiles: vec![
                AudioCardProfile {
                    name: "output:analog-stereo+input:analog-stereo".to_string(),
                    description: "Analog Stereo Duplex".to_string(),
                    available: true,
                },
                AudioCardProfile {
                    name: "off".to_string(),
                    description: "Off".to_string(),
                    available: true,
                },
            ],
            sinks: vec![AudioCardSink {
                sink_name: "alsa_output.pci-0000_00_1f.3.analog-stereo".to_string(),
                name: "Speakers".to_string(),
                icon: "audio-speakers-symbolic".to_string(),
                volume: 0.75,
                muted: false,
                default: true,
                active_port: Some("analog-output-speaker".to_string()),
                ports: vec![
                    AudioPort {
                        name: "analog-output-speaker".to_string(),
                        description: "Speaker".to_string(),
                        available: true,
                    },
                    AudioPort {
                        name: "analog-output-headphones".to_string(),
                        description: "Headphones".to_string(),
                        available: false,
                    },
                ],
            }],
            sources: vec![AudioCardSource {
                source_name: "alsa_input.pci-0000_00_1f.3.analog-stereo".to_string(),
                name: "Internal Microphone".to_string(),
                icon: "audio-input-microphone-symbolic".to_string(),
                volume: 0.5,
                muted: false,
                default: true,
                active_port: Some("analog-input-internal-mic".to_string()),
                ports: vec![AudioPort {
                    name: "analog-input-internal-mic".to_string(),
                    description: "Internal Microphone".to_string(),
                    available: true,
                }],
            }],
        };
        let json = serde_json::to_value(&card).unwrap();
        let decoded: AudioCard = serde_json::from_value(json).unwrap();
        assert_eq!(card, decoded);
    }

    #[test]
    fn audio_card_without_optional_fields() {
        let json = serde_json::json!({
            "name": "WH-1000XM4",
            "icon": "audio-headphones-symbolic",
            "active_profile": "a2dp-sink",
            "profiles": [],
            "sinks": [],
            "sources": []
        });
        let card: AudioCard = serde_json::from_value(json).unwrap();
        assert_eq!(card.connection_icon, None);
        assert!(card.sinks.is_empty());
        assert!(card.sources.is_empty());
    }
}
