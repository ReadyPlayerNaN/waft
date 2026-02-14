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

### Actions

| Action | Params | Description |
|--------|--------|-------------|
| `set-volume` | `{ "value": 0.75 }` | Set volume (0.0 to 1.0) |
| `toggle-mute` | none | Toggle mute state |
| `set-default` | none | Set as default device |

### URN Format

```
audio/audio-device/{device-name}
```

## System Interface

Uses the `pactl` command-line tool to interact with PulseAudio or PipeWire-Pulse. No direct D-Bus connection is required.

- `pactl list sinks` / `pactl list sources` -- enumerate devices
- `pactl list cards` -- card port info for display labels
- `pactl get-default-sink` / `pactl get-default-source` -- default device
- `pactl set-sink-volume` / `pactl set-source-volume` -- adjust volume
- `pactl set-sink-mute` / `pactl set-source-mute` -- mute control
- `pactl set-default-sink` / `pactl set-default-source` -- switch default
- `pactl subscribe` -- real-time event monitoring

## Dependencies

- **pactl** -- PulseAudio command-line tool (also works with PipeWire-Pulse)

## Configuration

```toml
[[plugins]]
id = "audio"
```

No plugin-specific configuration options.
