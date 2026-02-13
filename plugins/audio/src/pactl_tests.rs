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
        compute_primary_icon_sink(&icon_name, &None),
        "audio-headphones-symbolic"
    );
}

#[test]
fn test_compute_primary_icon_sink_defaults_to_speakers() {
    assert_eq!(
        compute_primary_icon_sink(&None, &None),
        "audio-speakers-symbolic"
    );
}

#[test]
fn test_compute_primary_icon_sink_avoids_double_symbolic_suffix() {
    let icon_name = Some("audio-headphones-symbolic".to_string());
    assert_eq!(
        compute_primary_icon_sink(&icon_name, &None),
        "audio-headphones-symbolic"
    );
}

#[test]
fn test_compute_primary_icon_sink_maps_audio_card_to_speakers() {
    let icon_name = Some("audio-card".to_string());
    assert_eq!(
        compute_primary_icon_sink(&icon_name, &None),
        "audio-speakers-symbolic"
    );
}

#[test]
fn test_compute_primary_icon_sink_maps_audio_headset_to_headphones() {
    let icon_name = Some("audio-headset".to_string());
    assert_eq!(
        compute_primary_icon_sink(&icon_name, &None),
        "audio-headphones-symbolic"
    );
}

#[test]
fn test_compute_primary_icon_sink_maps_video_display() {
    let icon_name = Some("video-display".to_string());
    assert_eq!(
        compute_primary_icon_sink(&icon_name, &None),
        "video-display-symbolic"
    );
}

#[test]
fn test_compute_primary_icon_sink_port_headphones_overrides_icon_name() {
    let icon_name = Some("audio-card".to_string());
    let port = Some("analog-output-headphones".to_string());
    assert_eq!(
        compute_primary_icon_sink(&icon_name, &port),
        "audio-headphones-symbolic"
    );
}

#[test]
fn test_compute_primary_icon_sink_port_headset_overrides_icon_name() {
    let icon_name = Some("audio-card".to_string());
    let port = Some("[Out] Headset".to_string());
    assert_eq!(
        compute_primary_icon_sink(&icon_name, &port),
        "audio-headphones-symbolic"
    );
}

#[test]
fn test_compute_primary_icon_sink_port_hdmi() {
    let icon_name = Some("audio-card".to_string());
    let port = Some("[Out] HDMI1".to_string());
    assert_eq!(
        compute_primary_icon_sink(&icon_name, &port),
        "video-display-symbolic"
    );
}

#[test]
fn test_compute_primary_icon_sink_port_speaker() {
    let icon_name = Some("audio-card".to_string());
    let port = Some("analog-output-speaker".to_string());
    assert_eq!(
        compute_primary_icon_sink(&icon_name, &port),
        "audio-speakers-symbolic"
    );
}

#[test]
fn test_compute_primary_icon_sink_port_lineout() {
    let icon_name = Some("audio-card-analog".to_string());
    let port = Some("analog-output-lineout".to_string());
    assert_eq!(
        compute_primary_icon_sink(&icon_name, &port),
        "audio-speakers-symbolic"
    );
}

#[test]
fn test_compute_primary_icon_sink_pipewire_audio_card_analog() {
    let icon_name = Some("audio-card-analog".to_string());
    assert_eq!(
        compute_primary_icon_sink(&icon_name, &None),
        "audio-speakers-symbolic"
    );
}

#[test]
fn test_compute_primary_icon_sink_pipewire_audio_headset_bluetooth() {
    let icon_name = Some("audio-headset-bluetooth".to_string());
    assert_eq!(
        compute_primary_icon_sink(&icon_name, &None),
        "audio-headphones-symbolic"
    );
}

#[test]
fn test_compute_primary_icon_sink_pipewire_audio_card_pci() {
    let icon_name = Some("audio-card-pci".to_string());
    assert_eq!(
        compute_primary_icon_sink(&icon_name, &None),
        "audio-speakers-symbolic"
    );
}

#[test]
fn test_compute_primary_icon_source_uses_icon_name() {
    let icon_name = Some("audio-headset".to_string());
    assert_eq!(
        compute_primary_icon_source(&icon_name, &None),
        "audio-headphones-symbolic"
    );
}

#[test]
fn test_compute_primary_icon_source_defaults_to_microphone() {
    assert_eq!(
        compute_primary_icon_source(&None, &None),
        "audio-input-microphone-symbolic"
    );
}

#[test]
fn test_compute_primary_icon_source_avoids_double_symbolic_suffix() {
    let icon_name = Some("audio-headset-symbolic".to_string());
    assert_eq!(
        compute_primary_icon_source(&icon_name, &None),
        "audio-headset-symbolic"
    );
}

#[test]
fn test_compute_primary_icon_source_maps_audio_card_to_microphone() {
    let icon_name = Some("audio-card".to_string());
    assert_eq!(
        compute_primary_icon_source(&icon_name, &None),
        "audio-input-microphone-symbolic"
    );
}

#[test]
fn test_compute_primary_icon_source_maps_camera_web() {
    let icon_name = Some("camera-web".to_string());
    assert_eq!(
        compute_primary_icon_source(&icon_name, &None),
        "camera-web-symbolic"
    );
}

#[test]
fn test_compute_primary_icon_source_pipewire_audio_card_analog() {
    let icon_name = Some("audio-card-analog".to_string());
    assert_eq!(
        compute_primary_icon_source(&icon_name, &None),
        "audio-input-microphone-symbolic"
    );
}

#[test]
fn test_compute_primary_icon_source_pipewire_audio_headset_bluetooth() {
    let icon_name = Some("audio-headset-bluetooth".to_string());
    assert_eq!(
        compute_primary_icon_source(&icon_name, &None),
        "audio-headphones-symbolic"
    );
}

#[test]
fn test_compute_primary_icon_source_port_headset_overrides_icon_name() {
    let icon_name = Some("audio-card".to_string());
    let port = Some("analog-input-headset-mic".to_string());
    assert_eq!(
        compute_primary_icon_source(&icon_name, &port),
        "audio-headphones-symbolic"
    );
}

#[test]
fn test_compute_primary_icon_source_port_webcam() {
    let icon_name = Some("audio-card".to_string());
    let port = Some("[In] Webcam".to_string());
    assert_eq!(
        compute_primary_icon_source(&icon_name, &port),
        "camera-web-symbolic"
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
    assert_eq!(device.icon, "audio-speakers-symbolic");
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

// ---------------------------------------------------------------------------
// Multi-device parsing: verify icon_name and active_port are parsed for ALL
// sinks/sources, not just the default.
// ---------------------------------------------------------------------------

const MULTI_SINK_OUTPUT: &str = "\
Sink #49
	State: RUNNING
	Name: alsa_output.pci-0000_00_1f.3.analog-stereo
	Description: Built-in Audio Analog Stereo
	Volume: front-left: 32768 / 50% / -18.06 dB, front-right: 32768 / 50% / -18.06 dB
	Mute: no
	Properties:
		device.icon_name = \"audio-card\"
		device.bus = \"pci\"
		node.nick = \"Speakers\"
		device.id = \"49\"
	Ports:
		analog-output-speaker: Speaker (type: Speaker, priority: 100, availability group: Legacy 2, available)
		analog-output-headphones: Headphones (type: Headphones, priority: 200, availability group: Legacy 1, not available)
	Active Port: analog-output-speaker

Sink #62
	State: SUSPENDED
	Name: bluez_output.AA_BB_CC_DD_EE_FF.1
	Description: WH-1000XM4
	Volume: front-left: 65536 / 100% / 0.00 dB, front-right: 65536 / 100% / 0.00 dB
	Mute: no
	Properties:
		device.icon_name = \"audio-headphones\"
		device.bus = \"bluetooth\"
		node.nick = \"WH-1000XM4\"
		device.id = \"62\"
	Ports:
		headset-output: Headset (type: Headset, priority: 0, availability group: , available)
	Active Port: headset-output

Sink #75
	State: IDLE
	Name: alsa_output.pci-0000_01_00.1.hdmi-stereo
	Description: HDMI Audio
	Volume: front-left: 65536 / 100% / 0.00 dB, front-right: 65536 / 100% / 0.00 dB
	Mute: no
	Properties:
		device.icon_name = \"video-display\"
		device.bus = \"pci\"
		node.nick = \"HDMI Output\"
		device.id = \"75\"
	Ports:
		[Out] HDMI1: HDMI / DisplayPort (type: HDMI, priority: 5900, availability group: Legacy 1, available)
	Active Port: [Out] HDMI1
";

#[test]
fn test_parse_sinks_all_devices_get_icon_name_and_active_port() {
    let sinks =
        parse_sinks(MULTI_SINK_OUTPUT, Some("alsa_output.pci-0000_00_1f.3.analog-stereo"))
            .unwrap();

    assert_eq!(sinks.len(), 3);

    // Default sink -- speaker port
    assert_eq!(sinks[0].name, "alsa_output.pci-0000_00_1f.3.analog-stereo");
    assert!(sinks[0].is_default);
    assert_eq!(sinks[0].icon_name.as_deref(), Some("audio-card"));
    assert_eq!(
        sinks[0].active_port.as_deref(),
        Some("analog-output-speaker")
    );

    // Non-default bluetooth headphones
    assert_eq!(sinks[1].name, "bluez_output.AA_BB_CC_DD_EE_FF.1");
    assert!(!sinks[1].is_default);
    assert_eq!(sinks[1].icon_name.as_deref(), Some("audio-headphones"));
    assert_eq!(sinks[1].active_port.as_deref(), Some("headset-output"));
    assert_eq!(sinks[1].bus.as_deref(), Some("bluetooth"));

    // Non-default HDMI output
    assert_eq!(sinks[2].name, "alsa_output.pci-0000_01_00.1.hdmi-stereo");
    assert!(!sinks[2].is_default);
    assert_eq!(sinks[2].icon_name.as_deref(), Some("video-display"));
    assert_eq!(sinks[2].active_port.as_deref(), Some("[Out] HDMI1"));
}

#[test]
fn test_non_default_sink_icons_computed_correctly() {
    let sinks =
        parse_sinks(MULTI_SINK_OUTPUT, Some("alsa_output.pci-0000_00_1f.3.analog-stereo"))
            .unwrap();

    let card_ports = CardPortMap::new();

    // Default sink: active port "analog-output-speaker" -> speaker icon
    let default_device = AudioDevice::from_sink(&sinks[0], &card_ports);
    assert_eq!(default_device.icon, "audio-speakers-symbolic");
    assert_eq!(default_device.secondary_icon, None);

    // Non-default bluetooth: icon_name "audio-headphones" -> headphones icon
    let bt_device = AudioDevice::from_sink(&sinks[1], &card_ports);
    assert_eq!(bt_device.icon, "audio-headphones-symbolic");
    assert_eq!(
        bt_device.secondary_icon,
        Some("bluetooth-symbolic".to_string())
    );

    // Non-default HDMI: active port "[Out] HDMI1" -> video-display icon
    let hdmi_device = AudioDevice::from_sink(&sinks[2], &card_ports);
    assert_eq!(hdmi_device.icon, "video-display-symbolic");
    assert_eq!(
        hdmi_device.secondary_icon,
        Some("video-joined-displays-symbolic".to_string())
    );
}

#[test]
fn test_non_default_sink_icons_after_switching_default() {
    // After switching default to the bluetooth device, the old default should
    // still have correct icon_name parsed from pactl output
    let sinks = parse_sinks(
        MULTI_SINK_OUTPUT,
        Some("bluez_output.AA_BB_CC_DD_EE_FF.1"),
    )
    .unwrap();

    let card_ports = CardPortMap::new();

    // Previously-default speaker is now non-default -- should still have speaker icon
    let speaker_device = AudioDevice::from_sink(&sinks[0], &card_ports);
    assert!(!sinks[0].is_default);
    assert_eq!(speaker_device.icon, "audio-speakers-symbolic");

    // New default bluetooth headphones
    let bt_device = AudioDevice::from_sink(&sinks[1], &card_ports);
    assert!(sinks[1].is_default);
    assert_eq!(bt_device.icon, "audio-headphones-symbolic");
    assert_eq!(
        bt_device.secondary_icon,
        Some("bluetooth-symbolic".to_string())
    );
}

const MULTI_SOURCE_OUTPUT: &str = "\
Source #50
	State: RUNNING
	Name: alsa_input.pci-0000_00_1f.3.analog-stereo
	Description: Built-in Audio Analog Stereo
	Volume: front-left: 65536 / 100% / 0.00 dB, front-right: 65536 / 100% / 0.00 dB
	Mute: no
	Properties:
		device.icon_name = \"audio-card\"
		device.bus = \"pci\"
		node.nick = \"Internal Microphone\"
		device.id = \"50\"
	Ports:
		analog-input-internal-mic: Internal Microphone (type: Mic, priority: 100, available)
	Active Port: analog-input-internal-mic

Source #63
	State: SUSPENDED
	Name: bluez_input.AA_BB_CC_DD_EE_FF.0
	Description: WH-1000XM4
	Volume: front-left: 65536 / 100% / 0.00 dB, front-right: 65536 / 100% / 0.00 dB
	Mute: no
	Properties:
		device.icon_name = \"audio-headset\"
		device.bus = \"bluetooth\"
		node.nick = \"WH-1000XM4\"
		device.id = \"63\"
	Ports:
		headset-input: Headset (type: Headset, priority: 0, availability group: , available)
	Active Port: headset-input

Source #80
	State: SUSPENDED
	Name: alsa_input.usb-Webcam_C920-02.analog-stereo
	Description: C920 HD Pro Webcam
	Volume: front-left: 65536 / 100% / 0.00 dB, front-right: 65536 / 100% / 0.00 dB
	Mute: no
	Properties:
		device.icon_name = \"camera-web\"
		device.bus = \"usb\"
		node.nick = \"C920\"
		device.id = \"80\"
	Ports:
		analog-input: Analog Input (type: Mic, priority: 100, available)
	Active Port: analog-input
";

#[test]
fn test_parse_sources_all_devices_get_icon_name_and_active_port() {
    let sources = parse_sources(
        MULTI_SOURCE_OUTPUT,
        Some("alsa_input.pci-0000_00_1f.3.analog-stereo"),
    )
    .unwrap();

    assert_eq!(sources.len(), 3);

    // Default source -- internal mic
    assert_eq!(sources[0].name, "alsa_input.pci-0000_00_1f.3.analog-stereo");
    assert!(sources[0].is_default);
    assert_eq!(sources[0].icon_name.as_deref(), Some("audio-card"));

    // Non-default bluetooth headset
    assert_eq!(sources[1].name, "bluez_input.AA_BB_CC_DD_EE_FF.0");
    assert!(!sources[1].is_default);
    assert_eq!(sources[1].icon_name.as_deref(), Some("audio-headset"));
    assert_eq!(sources[1].bus.as_deref(), Some("bluetooth"));

    // Non-default webcam
    assert_eq!(
        sources[2].name,
        "alsa_input.usb-Webcam_C920-02.analog-stereo"
    );
    assert!(!sources[2].is_default);
    assert_eq!(sources[2].icon_name.as_deref(), Some("camera-web"));
}

#[test]
fn test_non_default_source_icons_computed_correctly() {
    let sources = parse_sources(
        MULTI_SOURCE_OUTPUT,
        Some("alsa_input.pci-0000_00_1f.3.analog-stereo"),
    )
    .unwrap();

    let card_ports = CardPortMap::new();

    // Default source: internal mic with audio-card icon
    let default_device = AudioDevice::from_source(&sources[0], &card_ports);
    assert_eq!(default_device.icon, "audio-input-microphone-symbolic");

    // Non-default bluetooth headset
    let bt_device = AudioDevice::from_source(&sources[1], &card_ports);
    assert_eq!(bt_device.icon, "audio-headphones-symbolic");
    assert_eq!(
        bt_device.secondary_icon,
        Some("bluetooth-symbolic".to_string())
    );

    // Non-default webcam
    let webcam_device = AudioDevice::from_source(&sources[2], &card_ports);
    assert_eq!(webcam_device.icon, "camera-web-symbolic");
    assert_eq!(webcam_device.secondary_icon, None);
}

#[test]
fn test_port_availability_parsed_for_all_sinks() {
    let sinks =
        parse_sinks(MULTI_SINK_OUTPUT, Some("alsa_output.pci-0000_00_1f.3.analog-stereo"))
            .unwrap();

    // Default sink has active port "analog-output-speaker" which is "available"
    assert_eq!(sinks[0].active_port_available, Some(true));

    // Bluetooth sink has "headset-output" which is "available"
    assert_eq!(sinks[1].active_port_available, Some(true));

    // HDMI sink has "[Out] HDMI1" which is "available"
    assert_eq!(sinks[2].active_port_available, Some(true));
}
