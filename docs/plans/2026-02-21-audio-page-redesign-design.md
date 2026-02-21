# Audio Page Redesign — Design Document

**Date:** 2026-02-21
**Branch:** `larger-larger-picture`

## Overview

Three focused improvements to the audio page in `waft-settings`:

1. **Card header icon** — device-type icon in the `adw::PreferencesGroup` header suffix (right side)
2. **Card visual grouping** — the `adw::PreferencesGroup` boxed-list style already provides the card frame; ensure it is surfaced correctly
3. **Port rows always visible** — replace the per-sink/source `adw::ComboRow` port selector with individual `adw::ActionRow` port rows, always shown, displaying connected/disconnected status

---

## Files Changed

| File | Change |
|------|--------|
| `crates/settings/src/audio/device_card.rs` | Main implementation changes |
| `crates/i18n/locales/en/settings.ftl` | Add `audio-port-connected`, `audio-port-disconnected` keys |
| `crates/i18n/locales/cs/settings.ftl` | Czech translations for above keys |

---

## Design

### 1. Card Header Icon

In `AudioDeviceCard::new()`, after building the `adw::PreferencesGroup`, call:

```rust
let header_icon = IconWidget::from_name(
    audio_device_icon(&card.device_type, AudioDeviceKind::Output),
    24,
);
root.set_header_suffix(Some(header_icon.widget()));
```

In `apply_props()`, update the icon when the card's device type might change (uncommon but possible):

```rust
// Re-resolve icon on every apply_props since device_type can change.
// The IconWidget is stored in the struct so we can call set_icon_name on it.
self.header_icon.set_icon_name(audio_device_icon(&card.device_type, AudioDeviceKind::Output));
```

`AudioDeviceCard` gains a `header_icon: IconWidget` field.

`audio_device_icon()` takes `(&str, AudioDeviceKind)`. For a card-level icon, pass `AudioDeviceKind::Output` as the convention — `device_type` (e.g. `"headset"`, `"card"`, `"display"`) is the primary discriminator and already encodes intent.

---

### 2. Port Rows — Struct Changes

#### `PortRow` (new private struct)

```rust
struct PortRow {
    root: adw::ActionRow,
    port_name: String,
    /// Trailing icon indicating connected/disconnected status.
    status_icon: gtk::Image,
    /// Trailing checkmark shown when this port is the active port.
    active_icon: gtk::Image,
}
```

`PortRow::new(port, active_port, sink_name_or_source_name, is_sink, output_cb)`:

- `root = adw::ActionRow::builder().title(&port.description).build()`
- `status_icon` — `gtk::Image` added as a trailing suffix widget
- `active_icon` — `gtk::Image::from_icon_name("object-select-symbolic")` added as trailing suffix widget (after `status_icon`)
- Connect `root.connect_activated(...)` → emit `SetSinkPort` / `SetSourcePort` (only fires when `root` is activatable)
- Call `apply(port, active_port)` at the end of `new()` to set initial state

`PortRow::apply(&self, port: &AudioPort, active_port: Option<&str>)`:

```
subtitle = if port.available { t("audio-port-connected") } else { t("audio-port-disconnected") }
root.set_subtitle(&subtitle)

if port.available:
    status_icon.set_icon_name("audio-card-symbolic")   // or similar plug/connect icon
    status_icon.remove_css_class("dim-label")
    root.set_activatable(true)
    root.remove_css_class("dim-label")
else:
    status_icon.set_icon_name("audio-card-symbolic")
    status_icon.add_css_class("dim-label")
    root.set_activatable(false)
    root.add_css_class("dim-label")

is_active = active_port == Some(port.name)
active_icon.set_visible(is_active)
```

Note: disconnected ports are visually dimmed and non-activatable (clicking does nothing). This matches the design decision — users cannot pre-select a disconnected port.

---

#### `SinkRow` / `SourceRow` — Field Changes

**Remove:**
- `port_row: adw::ComboRow`
- `port_model: gtk::StringList`
- `port_names: Rc<RefCell<Vec<String>>>`

**Add:**
- `port_rows: Vec<PortRow>`

The port rows are appended to `SinkRow.root` (the vertical `gtk::Box`) in order, after the slider row. Since this Box is already used as a container (the existing ComboRow was placed here the same way), the `adw::ActionRow` port rows will render within it using their own styling.

---

#### `AudioDeviceCard` — Field and Method Changes

**Remove from struct:**
- Nothing directly removed; `sinks_box` and `sources_box` remain

**Add to struct:**
- `header_icon: IconWidget`

**Remove methods:**
- `init_port_row()` — no longer needed
- `update_port_row()` — no longer needed

**Change in `build_sink_row()` / `build_source_row()`:**

Replace the ComboRow block with:

```rust
let port_rows: Vec<PortRow> = sink.ports.iter().map(|port| {
    PortRow::new(port, sink.active_port.as_deref(), &sink.sink_name, true, &self.output_cb)
}).collect();

for pr in &port_rows {
    root.append(pr.root.upcast_ref());
}
```

`SinkRow` stores `port_rows: Vec<PortRow>`.

---

### 3. Reconciliation

In `reconcile_sinks()` / `reconcile_sources()`, the port update block changes:

**Current:**
```rust
Self::update_port_row(&existing.port_row, ...);
existing.port_row.set_visible(sink.ports.len() > 1);
```

**New:**
```rust
if existing.port_rows.len() == sink.ports.len() {
    // In-place update — port count unchanged
    for (pr, port) in existing.port_rows.iter().zip(sink.ports.iter()) {
        pr.apply(port, sink.active_port.as_deref());
    }
} else {
    // Structural change — rebuild port rows
    for pr in &existing.port_rows {
        existing.root.remove(pr.root.upcast_ref());
    }
    existing.port_rows = sink.ports.iter().map(|port| {
        let pr = PortRow::new(port, sink.active_port.as_deref(), &sink.sink_name, true, &self.output_cb);
        existing.root.append(pr.root.upcast_ref());
        pr
    }).collect();
}
```

The `updating` guard is no longer needed for ports (no combo signal to suppress).

---

### 4. i18n Keys

Add to `crates/i18n/locales/en/settings.ftl`:

```ftl
audio-port-connected = Connected
audio-port-disconnected = Disconnected
```

Add corresponding Czech translations to `crates/i18n/locales/cs/settings.ftl`:

```ftl
audio-port-connected = Připojeno
audio-port-disconnected = Odpojeno
```

---

## Non-Changes

- `AudioDeviceCardOutput` enum: unchanged (still has `SetSinkPort` / `SetSourcePort` variants)
- `AudioCard` / `AudioPort` protocol types: unchanged
- Volume slider interaction tracking (debounce, gesture, signal blocking): unchanged
- Mute button / default button logic: unchanged
- Profile ComboRow: unchanged
- Section labels (Output / Input): unchanged

---

## Implementation Order

1. Add i18n keys (en + cs)
2. Add `PortRow` struct and its `new()` / `apply()` methods
3. Update `SinkRow` / `SourceRow` structs (remove ComboRow fields, add `port_rows`)
4. Update `build_sink_row()` / `build_source_row()` to create PortRows
5. Update `reconcile_sinks()` / `reconcile_sources()` for in-place vs rebuild
6. Add `header_icon` field to `AudioDeviceCard`, wire in `new()` and `apply_props()`
7. Remove `init_port_row()` and `update_port_row()` helper functions
8. `cargo build -p waft-settings` and `cargo test -p waft-settings`
