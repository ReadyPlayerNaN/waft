# Audio Plugin

Volume control plugin with expandable device selection menus for audio output (speakers) and input (microphone).

## Features

- **Volume Sliders**: Separate sliders for output and input audio
- **Mute Toggle**: Click the icon to mute/unmute
- **Device Selection**: Expandable menus to select default audio devices
- **Real-time Updates**: Automatically reflects changes made by other applications
- **PulseAudio/PipeWire Support**: Works with both audio systems via `pactl`

## Requirements

- PulseAudio or PipeWire with PulseAudio compatibility (`pipewire-pulse`)
- `pactl` command available in PATH

## Configuration

Enable the plugin in `~/.config/waft-overview/config.toml`:

```toml
[[plugins]]
id = "plugin::audio"
```

## UI Components

### Audio Slider

```
┌─────────────────────────────────────────────┐
│  🔊  ═══════════════════════════●═══════  ▼ │
└─────────────────────────────────────────────┘
  │              │                            │
  │              │                            └─ Expand button (show devices)
  │              └─ Volume slider (0-100%)
  └─ Mute toggle (click to mute/unmute)
```

### Device Menu (expanded)

```
┌─────────────────────────────────────────────┐
│  🔊  Built-in Audio Analog Stereo        ✓  │
│  🔊  HDMI Output                             │
│  🔊  USB Headset                             │
└─────────────────────────────────────────────┘
```

## Interactions

| Action | Result |
|--------|--------|
| Click icon | Toggle mute |
| Drag slider | Change volume |
| Click expand button | Show/hide device menu |
| Click device | Set as default |

## Icons

- Output: `audio-volume-high-symbolic` (muted: `audio-volume-muted-symbolic`)
- Input: `audio-input-microphone-symbolic` (muted: `audio-input-microphone-muted-symbolic`)

## CSS Classes

| Class | Description |
|-------|-------------|
| `.slider-control` | Root slider container |
| `.slider-row` | Slider row (icon + scale + expand button) |
| `.slider-row.muted` | When audio is muted |
| `.slider-row.expanded` | When device menu is visible |
| `.slider-icon` | Mute toggle button |
| `.slider-scale` | Volume slider |
| `.slider-expand` | Expand/collapse button |
| `.slider-menu-container` | Expandable menu container |
| `.audio-device-menu` | Device list container |
| `.audio-device-row` | Individual device entry |
| `.audio-device-row.default` | Currently selected device |

## Architecture

```
mod.rs              Plugin entry point, UI orchestration
├── store.rs            State management (AudioStore, AudioState, AudioOp)
├── control_widget.rs   Self-contained control (slider + expandable device menu)
├── device_menu.rs      Device selection menu widget
└── dbus.rs             PulseAudio integration via pactl
```

The plugin exports widgets to `Slot::Controls`:
- Output control: Speaker volume slider with output device menu
- Input control: Microphone volume slider with input device menu

Each control is self-contained with its own revealer for the device menu expansion.

## Implementation Notes

- Uses `pactl` CLI for maximum compatibility with both PulseAudio and PipeWire
- Subscribes to `pactl subscribe` for real-time event monitoring
- Monitor sources (`.monitor` suffix) are filtered out from input devices
- Volume is normalized to 0.0-1.0 range internally
