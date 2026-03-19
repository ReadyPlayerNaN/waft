//! Icon resolution for audio devices.
//!
//! Pure functions mapping semantic device_type/connection_type strings
//! (from the protocol) to GTK icon names. No widget state.

use waft_protocol::entity::audio::AudioDeviceKind;

/// Resolve a themed icon name for an audio device.
///
/// `device_type` is the semantic type string from the protocol entity
/// (e.g. "headset", "card", "display"). `kind` is used as a fallback
/// when device_type is unknown or generic.
pub fn audio_device_icon(device_type: &str, kind: AudioDeviceKind) -> &'static str {
    match device_type {
        "headset" => "audio-headset-symbolic",
        "headphone" | "hands-free" => "audio-headphones-symbolic",
        "webcam" => "camera-web-symbolic",
        "phone" => "phone-symbolic",
        "speaker" => "audio-speakers-symbolic",
        "microphone" => "audio-input-microphone-symbolic",
        "card" => "audio-card-symbolic",
        "display" => "video-display-symbolic",
        _ => match kind {
            AudioDeviceKind::Output => "audio-speakers-symbolic",
            AudioDeviceKind::Input => "audio-input-microphone-symbolic",
        },
    }
}

/// Resolve a connection badge icon for an audio device, if any.
///
/// Returns `None` for internal connections (PCI, virtual) that have no badge.
pub fn audio_connection_icon(connection_type: &str) -> Option<&'static str> {
    match connection_type {
        "bluetooth" => Some("bluetooth-symbolic"),
        "usb" => Some("media-removable-symbolic"),
        "jack" => Some("audio-jack-symbolic"),
        "hdmi" => Some("video-joined-displays-symbolic"),
        "virtual" => Some("applications-science-symbolic"),
        _ => None, // pci — no badge
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use waft_protocol::entity::audio::AudioDeviceKind;

    #[test]
    fn headset_always_shows_headset_icon() {
        assert_eq!(audio_device_icon("headset", AudioDeviceKind::Output), "audio-headset-symbolic");
        assert_eq!(audio_device_icon("headset", AudioDeviceKind::Input), "audio-headset-symbolic");
    }

    #[test]
    fn headphone_shows_headphones_icon() {
        assert_eq!(audio_device_icon("headphone", AudioDeviceKind::Output), "audio-headphones-symbolic");
    }

    #[test]
    fn hands_free_shows_headphones_icon() {
        assert_eq!(audio_device_icon("hands-free", AudioDeviceKind::Output), "audio-headphones-symbolic");
    }

    #[test]
    fn webcam_shows_camera_icon() {
        assert_eq!(audio_device_icon("webcam", AudioDeviceKind::Input), "camera-web-symbolic");
    }

    #[test]
    fn display_shows_video_display_icon() {
        assert_eq!(audio_device_icon("display", AudioDeviceKind::Output), "video-display-symbolic");
    }

    #[test]
    fn microphone_shows_microphone_icon() {
        assert_eq!(audio_device_icon("microphone", AudioDeviceKind::Input), "audio-input-microphone-symbolic");
    }

    #[test]
    fn card_shows_audio_card_icon() {
        assert_eq!(audio_device_icon("card", AudioDeviceKind::Output), "audio-card-symbolic");
    }

    #[test]
    fn speaker_shows_speakers_icon() {
        assert_eq!(audio_device_icon("speaker", AudioDeviceKind::Output), "audio-speakers-symbolic");
    }

    #[test]
    fn unknown_type_falls_back_to_direction() {
        assert_eq!(audio_device_icon("something-new", AudioDeviceKind::Output), "audio-speakers-symbolic");
        assert_eq!(audio_device_icon("something-new", AudioDeviceKind::Input), "audio-input-microphone-symbolic");
    }

    #[test]
    fn bluetooth_connection_icon() {
        assert_eq!(audio_connection_icon("bluetooth"), Some("bluetooth-symbolic"));
    }

    #[test]
    fn usb_connection_icon() {
        assert_eq!(audio_connection_icon("usb"), Some("media-removable-symbolic"));
    }

    #[test]
    fn jack_connection_icon() {
        assert_eq!(audio_connection_icon("jack"), Some("audio-jack-symbolic"));
    }

    #[test]
    fn hdmi_connection_icon() {
        assert_eq!(audio_connection_icon("hdmi"), Some("video-joined-displays-symbolic"));
    }

    #[test]
    fn pci_has_no_connection_badge() {
        assert_eq!(audio_connection_icon("pci"), None);
    }

    #[test]
    fn virtual_shows_science_badge() {
        assert_eq!(audio_connection_icon("virtual"), Some("applications-science-symbolic"));
    }
}
