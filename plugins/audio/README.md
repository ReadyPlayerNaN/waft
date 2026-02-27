# Audio Plugin

Volume control with device selection for PulseAudio and PipeWire systems.

## Purpose

Exposes audio output (sink) and input (source) devices as entities, allowing the overlay to display volume sliders and device selectors. Monitors PulseAudio/PipeWire events via `pactl subscribe` and pushes updates when audio state changes.

## Entity Types

### `audio-device`

One entity per audio device (both outputs and inputs).

| Field | Type | Description |
|-------|------|-------------|
| `name` | `String` | Human-readable device label |
| `icon` | `String` | Primary icon name (e.g. `audio-speakers-symbolic`) |
| `connection_icon` | `Option<String>` | Secondary icon for connection type (e.g. `bluetooth-symbolic`) |
| `volume` | `f64` | Volume level, 0.0 to 1.0 |
| `muted` | `bool` | Whether the device is muted |
| `default` | `bool` | Whether this is the default device |
| `kind` | `AudioDeviceKind` | `Output` or `Input` |

| `virtual_device` | `bool` | Whether this is a waft-managed virtual device |
| `sink_name` | `Option<String>` | Internal pactl sink/source name (for virtual device actions) |

### Actions

| Action | Params | Description |
|--------|--------|-------------|
| `set-volume` | `{ "value": 0.75 }` | Set volume (0.0 to 1.0) |
| `toggle-mute` | none | Toggle mute state |
| `set-default` | none | Set as default device |
| `create-sink` | `{ "label": "Display Name" }` | Create a virtual null-sink output device |
| `remove-sink` | `{ "sink_name": "waft_name" }` | Remove a virtual null-sink device |
| `create-source` | `{ "label": "Display Name" }` | Create a virtual null-source input device |
| `remove-source` | `{ "source_name": "waft_name" }` | Remove a virtual null-source device |

### URN Format

```
audio/audio-device/{device-name}
```

Virtual devices use URN `audio/audio-device/{sink_name}` where `sink_name` is the auto-generated `waft_`-prefixed name.

## System Interface

Uses the `pactl` command-line tool to interact with PulseAudio or PipeWire-Pulse. No direct D-Bus connection is required.

- `pactl list sinks` / `pactl list sources` -- enumerate devices
- `pactl list cards` -- card port info for display labels
- `pactl get-default-sink` / `pactl get-default-source` -- default device
- `pactl set-sink-volume` / `pactl set-source-volume` -- adjust volume
- `pactl set-sink-mute` / `pactl set-source-mute` -- mute control
- `pactl set-default-sink` / `pactl set-default-source` -- switch default
- `pactl subscribe` -- real-time event monitoring
- `pactl load-module` / `pactl unload-module` -- virtual device module management
- `pactl list modules short` -- list loaded modules for startup reconciliation

## Dependencies

- **pactl** -- PulseAudio command-line tool (also works with PipeWire-Pulse)

## Configuration

```toml
[[plugins]]
id = "audio"

[[plugins.virtual_devices]]
module_type = "null-sink"
sink_name = "waft_virtual_mic"
label = "Virtual Microphone"

[[plugins.virtual_devices]]
module_type = "null-source"
sink_name = "waft_virtual_source"
label = "Virtual Source"
```

### Virtual Devices

Virtual audio devices (null sinks and null sources) are managed through the settings UI or entity actions. Each device is persisted in two locations:

1. **`~/.config/waft/config.toml`** -- source of truth under the `[[plugins.virtual_devices]]` section
2. **`~/.config/pulse/default.pa`** -- `load-module` lines marked with `# waft-managed` so PulseAudio recreates them without waft running

On startup, the plugin reads the TOML config and reconciles with currently loaded PulseAudio modules. Missing devices are recreated via `pactl load-module`. The `default.pa` file is synced after every create/delete operation.

Sink names are auto-generated from user labels: lowercase, non-alphanumeric replaced with `_`, consecutive underscores collapsed, prefixed with `waft_`. Uniqueness is ensured by appending `_2`, `_3`, etc.

**PipeWire note:** PipeWire ignores `~/.config/pulse/default.pa`. On PipeWire systems, the TOML config plus waft startup reconciliation handles virtual device restoration.
