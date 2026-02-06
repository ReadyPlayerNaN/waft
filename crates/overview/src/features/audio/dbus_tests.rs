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

// Integration tests for pactl command execution would require:
// - A running PulseAudio/PipeWire instance
// - Mock pactl command or test doubles
// - Asynchronous test harness
//
// Future work: Add integration tests using test doubles or mock pactl output.
//
// Test scenarios to add:
// - get_card_port_info() parses pactl list cards output correctly
// - get_sinks() filters out unavailable ports
// - get_sources() filters out monitor devices
// - set_sink_volume() clamps volume to 0.0-1.0 range
// - set_default_sink() executes pactl command correctly
// - subscribe_events() parses event stream correctly
// - parse_sinks() handles multi-line pactl output
// - parse_sources() extracts all device properties
// - parse_card_ports() extracts product names from EDID data
