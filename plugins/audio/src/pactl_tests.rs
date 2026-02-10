use super::*;

#[test]
fn test_parse_volume_percent_extracts_first_percentage() {
    let output = "Volume: front-left: 65536 / 100% / 0.00 dB, front-right: 65536 / 100% / 0.00 dB";
    assert_eq!(parse_volume_percent(output), Some(1.0));
}

#[test]
fn test_parse_volume_percent_handles_partial_volume() {
    let output = "Volume: front-left: 32768 / 50% / -18.06 dB";
    assert_eq!(parse_volume_percent(output), Some(0.5));
}

#[test]
fn test_parse_volume_percent_returns_none_for_invalid() {
    let output = "Volume: no percentage here";
    assert_eq!(parse_volume_percent(output), None);
}

#[test]
fn test_parse_event_line_sink_change() {
    let line = "Event 'change' on sink #0";
    assert!(matches!(
        parse_event_line(line),
        Some(AudioEvent::Sink)
    ));
}

#[test]
fn test_parse_event_line_source_change() {
    let line = "Event 'change' on source #1";
    assert!(matches!(
        parse_event_line(line),
        Some(AudioEvent::Source)
    ));
}

#[test]
fn test_parse_event_line_server_change() {
    let line = "Event 'change' on server";
    assert!(matches!(
        parse_event_line(line),
        Some(AudioEvent::Server)
    ));
}

#[test]
fn test_parse_event_line_card_change() {
    let line = "Event 'change' on card #2";
    assert!(matches!(
        parse_event_line(line),
        Some(AudioEvent::Card)
    ));
}

#[test]
fn test_parse_event_line_ignores_sink_input() {
    let line = "Event 'change' on sink-input #5";
    assert_eq!(parse_event_line(line), None);
}

#[test]
fn test_parse_event_line_ignores_source_output() {
    let line = "Event 'change' on source-output #3";
    assert_eq!(parse_event_line(line), None);
}

#[test]
fn test_parse_property_line_extracts_key_value() {
    let line = "    device.icon_name = \"audio-headphones\"";
    assert_eq!(
        parse_property_line(line),
        Some(("device.icon_name", "audio-headphones"))
    );
}

#[test]
fn test_parse_property_line_handles_no_quotes() {
    let line = "    some.key = value";
    assert_eq!(parse_property_line(line), Some(("some.key", "value")));
}

#[test]
fn test_parse_property_line_returns_none_for_invalid() {
    let line = "Not a property line";
    assert_eq!(parse_property_line(line), None);
}

#[test]
fn test_compute_primary_icon_sink_uses_icon_name() {
    let icon_name = Some("audio-headphones".to_string());
    assert_eq!(
        compute_primary_icon_sink(&icon_name),
        "audio-headphones-symbolic"
    );
}

#[test]
fn test_compute_primary_icon_sink_defaults_to_speakers() {
    assert_eq!(compute_primary_icon_sink(&None), "audio-speakers-symbolic");
}

#[test]
fn test_compute_primary_icon_sink_avoids_double_symbolic_suffix() {
    let icon_name = Some("audio-headphones-symbolic".to_string());
    assert_eq!(
        compute_primary_icon_sink(&icon_name),
        "audio-headphones-symbolic"
    );
}

#[test]
fn test_compute_primary_icon_source_uses_icon_name() {
    let icon_name = Some("audio-headset".to_string());
    assert_eq!(
        compute_primary_icon_source(&icon_name),
        "audio-headset-symbolic"
    );
}

#[test]
fn test_compute_primary_icon_source_defaults_to_microphone() {
    assert_eq!(
        compute_primary_icon_source(&None),
        "audio-input-microphone-symbolic"
    );
}

#[test]
fn test_compute_primary_icon_source_avoids_double_symbolic_suffix() {
    let icon_name = Some("audio-headset-symbolic".to_string());
    assert_eq!(
        compute_primary_icon_source(&icon_name),
        "audio-headset-symbolic"
    );
}

#[test]
fn test_compute_secondary_icon_video_display() {
    let icon_name = Some("video-display".to_string());
    let bus = None;
    assert_eq!(
        compute_secondary_icon(&icon_name, &bus),
        Some("video-joined-displays-symbolic".to_string())
    );
}

#[test]
fn test_compute_secondary_icon_bluetooth() {
    let icon_name = None;
    let bus = Some("bluetooth".to_string());
    assert_eq!(
        compute_secondary_icon(&icon_name, &bus),
        Some("bluetooth-symbolic".to_string())
    );
}

#[test]
fn test_compute_secondary_icon_none_for_regular_device() {
    let icon_name = Some("audio-headphones".to_string());
    let bus = Some("usb".to_string());
    assert_eq!(compute_secondary_icon(&icon_name, &bus), None);
}

#[test]
fn test_muted_icon_appends_muted() {
    assert_eq!(
        muted_icon("audio-volume-high-symbolic"),
        "audio-volume-high-muted-symbolic"
    );
}

#[test]
fn test_muted_icon_without_symbolic_suffix() {
    assert_eq!(
        muted_icon("audio-speakers"),
        "audio-speakers-muted-symbolic"
    );
}

#[test]
fn test_audio_device_from_sink() {
    let card_ports = CardPortMap::new();
    let sink = SinkInfo {
        name: "alsa_output.pci-0000_00_1f.3.analog-stereo".to_string(),
        description: "Built-in Audio Analog Stereo".to_string(),
        volume_percent: 0.75,
        muted: false,
        is_default: true,
        icon_name: Some("audio-card".to_string()),
        bus: Some("pci".to_string()),
        node_nick: Some("Speakers".to_string()),
        device_id: None,
        active_port: None,
        active_port_available: None,
    };

    let device = AudioDevice::from_sink(&sink, &card_ports);
    assert_eq!(device.id, "alsa_output.pci-0000_00_1f.3.analog-stereo");
    assert_eq!(device.name, "Speakers");
    assert_eq!(device.icon, "audio-card-symbolic");
    assert_eq!(device.secondary_icon, None);
}

#[test]
fn test_audio_device_from_source() {
    let card_ports = CardPortMap::new();
    let source = SourceInfo {
        name: "alsa_input.pci-0000_00_1f.3.analog-stereo".to_string(),
        description: "Built-in Audio Analog Stereo".to_string(),
        volume_percent: 0.5,
        muted: false,
        is_default: true,
        icon_name: None,
        bus: None,
        node_nick: None,
        device_id: None,
        active_port: None,
        active_port_available: None,
    };

    let device = AudioDevice::from_source(&source, &card_ports);
    assert_eq!(device.id, "alsa_input.pci-0000_00_1f.3.analog-stereo");
    assert_eq!(device.name, "Built-in Audio Analog Stereo");
    assert_eq!(device.icon, "audio-input-microphone-symbolic");
    assert_eq!(device.secondary_icon, None);
}
