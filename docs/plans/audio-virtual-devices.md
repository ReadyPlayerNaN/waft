# Virtual Audio Devices -- Implementation Plan

## Current State Summary

The virtual audio device feature is **already partially implemented** across all three layers.

**Protocol** (`crates/protocol/src/entity/audio.rs`):
- `AudioDevice` has `virtual_device: bool` and `sink_name: Option<String>` fields
- No separate entity type; virtual devices use the same `audio-device` entity type

**Plugin** (`plugins/audio/bin/waft-audio-daemon.rs` and `plugins/audio/src/`):
- `VirtualDeviceConfig` persists definitions to `~/.config/waft/config.toml`
- `sync_default_pa()` writes `~/.config/pulse/default.pa` load-module lines so devices survive reboots
- `reconcile_virtual_devices()` on startup loads missing PulseAudio modules (module-null-sink / module-null-source)
- Actions implemented: `create-sink`, `create-source`, `remove-sink`, `remove-source`
- Name sanitization (`waft_` prefix, alphanumeric-only) with uniqueness enforcement

**Settings UI** (`crates/settings/src/audio/virtual_devices_section.rs`):
- `VirtualDevicesSection` smart container renders virtual device rows with volume sliders, mute buttons, and delete buttons
- Create dialog with type selector (Output Sink / Input Source) and name entry
- Interaction tracking with debounce for volume sliders
- Empty state with informative description
- Localized in both en-US and cs-CZ

---

## Key Bug: Hardcoded Volume/Mute for Virtual Devices

The biggest existing bug is in the plugin's `get_entities()` method (line ~320 of `waft-audio-daemon.rs`):

```rust
volume: 1.0,
muted: false,
```

Virtual device entities are emitted with **hardcoded** volume and mute values instead of reading actual state from pactl. When a user adjusts volume via the settings slider, `set-volume` calls `pactl::set_sink_volume` and pactl events fire, but the entity gets regenerated with the same hardcoded `1.0`/`false` values. The slider snaps back to full volume after reconciliation.

Additionally, virtual sinks/sources appear in `pactl list sinks`/`pactl list sources` output, so they exist in `state.sinks`/`state.sources`. This means they get emitted **twice** as `audio-device` entities -- once from the regular device loop with real volume, and once from the virtual device loop with hardcoded volume.

---

## What "Virtual Audio Devices" Means

Virtual audio devices in PulseAudio/PipeWire are software-only audio endpoints:

- **module-null-sink**: Virtual output sink. Audio sent to it is discarded (or captured via `.monitor` source). Use cases: routing audio between apps, screen recording capture, testing.
- **module-null-source**: Virtual input source. Produces silence, can be used as a routing target. Use cases: virtual microphone for voice changers, routing audio from one app as mic input for another.
- **module-loopback**: Connects source to sink for real-time routing. Not currently implemented.

Current implementation only supports null-sink and null-source, which covers primary use cases.

---

## What Needs to Be Fixed / Implemented

### 1. Fix Virtual Device Volume/Mute Tracking (High Priority, Bug Fix)

**File:** `plugins/audio/bin/waft-audio-daemon.rs`

**Suggested approach:** In the output/input device entity loops, check if `device.id` matches any `virtual_devices[].config.sink_name`. If so, set `virtual_device: true` and `sink_name: Some(...)` on the entity. Then **remove the separate "Virtual device entities" loop entirely**.

This ensures virtual devices get real volume, mute, and default state from pactl without cross-referencing, and eliminates duplicate entities.

### 2. Volume/Mute Actions (Likely Already Working)

The `handle_action` for `set-volume` already calls `pactl::set_sink_volume(&device_id, volume)` where `device_id` is the URN id. For virtual devices integrated into regular lists, this should work seamlessly. The `toggle-mute` action also looks up devices in `state.output_devices`/`state.input_devices`, so if virtual devices are in those lists, mute works too.

### 3. No Protocol Changes Needed

The protocol already has the necessary fields (`virtual_device`, `sink_name`). No new entity types or fields required.

### 4. No Major UI Changes Needed

The `VirtualDevicesSection` is well-implemented. Only needs verification after the plugin fix.

### 5. Minor UI Polish (Optional)

- The create dialog's button label is hardcoded to `t("audio-create-virtual-sink")` regardless of type selection. Should dynamically switch to `t("audio-create-virtual-source")` when "Input Source" is selected.
- Could validate that the label won't collide with existing device names.

---

## Suggested Implementation

### Phase 1: Fix the Volume/Mute Bug

1. In `get_entities()`, modify output device loop to detect virtual devices by checking `device.id` against `virtual_devices[].config.sink_name`. If matched, set `virtual_device: true` and `sink_name`.
2. Same for input device loop.
3. Remove the separate "Virtual device entities" loop (lines ~310-332).
4. Virtual devices now get real volume, mute, and default state from pactl.

### Phase 2: Fix the Create Dialog Label (Minor polish)

1. Wire `connect_selected_notify` on `type_combo` to update the "create" response button label dynamically.

### Phase 3: Test End-to-End

1. Create a virtual sink via settings UI
2. Verify it appears with correct volume
3. Adjust volume -- verify slider reflects changes on reconciliation
4. Toggle mute -- verify icon updates
5. Delete -- verify it disappears
6. Restart waft -- verify device recreated from config
7. Verify appearance/absence in overview (based on desired behavior)

---

## Questions Requiring User Input

1. **Should virtual devices appear in the overview (audio sliders)?** They use the same `audio-device` entity type. The `virtual_device` flag is available to filter them. What is the intended behavior?

2. **Should virtual devices support being set as default output/input?** Current UI doesn't have "set as default" for virtual devices. Should it?

3. **Should monitor sources (e.g., `waft_my_sink.monitor`) be exposed?** Currently filtered out. Should they remain hidden?

4. **Is loopback module support desired for this iteration?** `module-loopback` connects source to sink for real-time routing. Requires complex UI (source/sink selection). Defer?

5. **Confirmation before deletion?** When a virtual device is deleted, PulseAudio moves streams to the default device. Should there be a confirmation dialog?

6. **Persistence on PipeWire:** Current implementation writes to `~/.config/pulse/default.pa`. On PipeWire systems, this may not be relevant. Should there be a PipeWire-specific persistence path (WirePlumber)?

---

## Critical Files

- `plugins/audio/bin/waft-audio-daemon.rs` - Core fix: virtual device entity generation
- `crates/settings/src/audio/virtual_devices_section.rs` - UI (already implemented, minor polish)
- `plugins/audio/src/virtual_device_config.rs` - Persistence (already implemented, no changes needed)
- `crates/protocol/src/entity/audio.rs` - Protocol (already has fields, no changes needed)
- `crates/settings/src/pages/audio.rs` - Audio page wiring (already implemented)
