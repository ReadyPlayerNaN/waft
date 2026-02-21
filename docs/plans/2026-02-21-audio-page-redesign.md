# Audio Page Redesign Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Improve the audio settings page with a device-type icon in each card header and individual always-visible port rows that show connected/disconnected status.

**Architecture:** The `AudioDeviceCard` dumb widget in `crates/settings/src/audio/device_card.rs` is modified in-place. The per-sink/source `adw::ComboRow` port selector is replaced by a new `PortRow` struct (individual `adw::ActionRow` per port). The `adw::PreferencesGroup` header gets a device-type icon via `set_header_suffix()`.

**Tech Stack:** Rust, GTK4, libadwaita (`adw::PreferencesGroup`, `adw::ActionRow`), Fluent i18n, `waft_ui_gtk::widgets::icon::IconWidget`, `waft_ui_gtk::audio::icon::audio_device_icon`

**Design doc:** `docs/plans/2026-02-21-audio-page-redesign-design.md`

---

### Task 1: Add i18n keys for port connected/disconnected status

**Files:**
- Modify: `crates/settings/locales/en-US/settings.ftl` (after line 40)
- Modify: `crates/settings/locales/cs-CZ/settings.ftl` (after line 40)

**Step 1: Add English keys**

In `crates/settings/locales/en-US/settings.ftl`, add after `audio-card-set-default-tooltip = Set { $name } as default`:

```ftl
audio-port-connected = Connected
audio-port-disconnected = Disconnected
```

**Step 2: Add Czech keys**

In `crates/settings/locales/cs-CZ/settings.ftl`, add after `audio-card-set-default-tooltip = Nastavit { $name } jako výchozí`:

```ftl
audio-port-connected = Připojeno
audio-port-disconnected = Odpojeno
```

Note: Czech uses UTF-8 diacritics — verify the file actually contains non-ASCII characters (`í`, `e`).

**Step 3: Build to confirm no missing key warnings**

```bash
cargo build -p waft-settings 2>&1 | head -30
```

Expected: compiles cleanly, no missing translation key warnings.

**Step 4: Commit**

```bash
git add crates/settings/locales/en-US/settings.ftl crates/settings/locales/cs-CZ/settings.ftl
git commit -m "feat(settings): add audio port connected/disconnected i18n keys"
```

---

### Task 2: Add `PortRow` struct

**Files:**
- Modify: `crates/settings/src/audio/device_card.rs`

Add this struct and impl **before** the existing `AudioDeviceCard` struct (after the `SourceRow` struct, around line 122). The `PortRow` type is private to the module.

**Step 1: Add the `PortRow` struct and `impl` block**

Insert between the closing `}` of `impl SourceRow` (line 151) and `/// A physical audio card widget` comment (line 153):

```rust
/// A port row widget for an individual sink or source port.
///
/// Always visible, shows connected/disconnected status and a checkmark
/// when this port is the active port. Activatable only when connected.
struct PortRow {
    root: adw::ActionRow,
    port_name: String,
    /// Trailing icon indicating connected/disconnected state (dimmed when disconnected).
    status_icon: IconWidget,
    /// Trailing checkmark icon — visible only when this port is the active port.
    active_icon: IconWidget,
}

impl PortRow {
    /// Create a new port row.
    ///
    /// `on_activate` is called with the port's internal name when the row is
    /// activated (clicked). The closure produces the correct `AudioDeviceCardOutput`
    /// variant (SetSinkPort or SetSourcePort) for the parent device.
    fn new<F>(
        port: &AudioPort,
        active_port: Option<&str>,
        on_activate: F,
        output_cb: &OutputCallback,
    ) -> Self
    where
        F: Fn(String) -> AudioDeviceCardOutput + 'static,
    {
        let row = adw::ActionRow::builder()
            .title(&port.description)
            .build();

        let status_icon = IconWidget::from_name("audio-card-symbolic", 16);
        row.add_suffix(status_icon.widget());

        let active_icon = IconWidget::from_name("object-select-symbolic", 16);
        row.add_suffix(active_icon.widget());

        // Wire activation: only fires when the row is activatable (i.e. port is connected).
        {
            let cb = output_cb.clone();
            let port_name = port.name.clone();
            row.connect_activated(move |_| {
                if let Some(ref callback) = *cb.borrow() {
                    callback(on_activate(port_name.clone()));
                }
            });
        }

        let this = Self {
            root: row,
            port_name: port.name.clone(),
            status_icon,
            active_icon,
        };
        this.apply(port, active_port);
        this
    }

    /// Update the row to reflect the current port state.
    fn apply(&self, port: &AudioPort, active_port: Option<&str>) {
        let subtitle = if port.available {
            t("audio-port-connected")
        } else {
            t("audio-port-disconnected")
        };
        self.root.set_subtitle(&subtitle);

        if port.available {
            self.status_icon.widget().remove_css_class("dim-label");
            self.root.set_activatable(true);
            self.root.remove_css_class("dim-label");
        } else {
            self.status_icon.widget().add_css_class("dim-label");
            self.root.set_activatable(false);
            self.root.add_css_class("dim-label");
        }

        let is_active = active_port == Some(self.port_name.as_str());
        self.active_icon.widget().set_visible(is_active);
    }
}
```

**Step 2: Build to confirm it compiles**

```bash
cargo build -p waft-settings 2>&1 | grep -E "^error"
```

Expected: no errors (the struct is unused yet — expect a dead_code warning, that's fine).

**Step 3: Commit**

```bash
git add crates/settings/src/audio/device_card.rs
git commit -m "feat(settings/audio): add PortRow struct for always-visible port status"
```

---

### Task 3: Replace ComboRow with PortRow in `SinkRow` / `SourceRow`

**Files:**
- Modify: `crates/settings/src/audio/device_card.rs`

This task replaces the `port_row: adw::ComboRow` / `port_model` / `port_names` fields on both `SinkRow` and `SourceRow` with `port_rows: Vec<PortRow>`, and rewires `build_sink_row()` / `build_source_row()` accordingly.

**Step 1: Update `SinkRow` struct (lines 78-102)**

Replace the entire `SinkRow` struct definition:

```rust
struct SinkRow {
    root: gtk::Box,
    sink_name: String,
    slider: gtk::Scale,
    slider_handler_id: Rc<glib::SignalHandlerId>,
    interacting: Rc<RefCell<bool>>,
    pending_value: Rc<RefCell<Option<f64>>>,
    #[allow(dead_code)]
    pointer_down: Rc<RefCell<bool>>,
    #[allow(dead_code)]
    debounce_source: Rc<RefCell<Option<glib::SourceId>>>,
    info_row: adw::ActionRow,
    mute_button: gtk::Button,
    default_button: gtk::Button,
    /// Individual port rows — always visible, one per port.
    port_rows: Vec<PortRow>,
}
```

**Step 2: Update `SourceRow` struct (lines 104-122)**

Replace the entire `SourceRow` struct definition:

```rust
struct SourceRow {
    root: gtk::Box,
    source_name: String,
    slider: gtk::Scale,
    slider_handler_id: Rc<glib::SignalHandlerId>,
    interacting: Rc<RefCell<bool>>,
    pending_value: Rc<RefCell<Option<f64>>>,
    #[allow(dead_code)]
    pointer_down: Rc<RefCell<bool>>,
    #[allow(dead_code)]
    debounce_source: Rc<RefCell<Option<glib::SourceId>>>,
    info_row: adw::ActionRow,
    mute_button: gtk::Button,
    default_button: gtk::Button,
    /// Individual port rows — always visible, one per port.
    port_rows: Vec<PortRow>,
}
```

**Step 3: Update `build_sink_row()` — port section**

In `build_sink_row()`, find the "Port combo row (if > 1 port)" block (lines 518-529) and the "Wire port combo" block (lines 696-716) and the `SinkRow { ... }` return (lines 718-733).

Replace the port combo block (lines 518-529):

```rust
        // Port rows — one adw::ActionRow per port, always visible.
        let port_rows: Vec<PortRow> = sink.ports.iter().map(|port| {
            let sink_name = sink.sink_name.clone();
            let pr = PortRow::new(
                port,
                sink.active_port.as_deref(),
                move |port_name| AudioDeviceCardOutput::SetSinkPort {
                    sink: sink_name.clone(),
                    port: port_name,
                },
                &self.output_cb,
            );
            root.append(pr.root.upcast_ref::<gtk::Widget>());
            pr
        }).collect();
```

Remove the entire "Wire port combo" block (lines 696-716) — it is replaced by the closure above.

Update the `SinkRow { ... }` return to remove the three port combo fields and add `port_rows`:

```rust
        SinkRow {
            root,
            sink_name: sink.sink_name.clone(),
            slider,
            slider_handler_id,
            interacting,
            pending_value,
            pointer_down,
            debounce_source,
            info_row,
            mute_button,
            default_button,
            port_rows,
        }
```

**Step 4: Update `build_source_row()` — port section**

Apply the same changes as Step 3 but for `SourceRow`:

Replace the port combo creation block (lines 806-820) with:

```rust
        // Port rows — one adw::ActionRow per port, always visible.
        let port_rows: Vec<PortRow> = source.ports.iter().map(|port| {
            let source_name = source.source_name.clone();
            let pr = PortRow::new(
                port,
                source.active_port.as_deref(),
                move |port_name| AudioDeviceCardOutput::SetSourcePort {
                    source: source_name.clone(),
                    port: port_name,
                },
                &self.output_cb,
            );
            root.append(pr.root.upcast_ref::<gtk::Widget>());
            pr
        }).collect();
```

Remove the "Wire port combo" block for sources (lines 977-997).

Update the `SourceRow { ... }` return to remove port combo fields and add `port_rows`:

```rust
        SourceRow {
            root,
            source_name: source.source_name.clone(),
            slider,
            slider_handler_id,
            interacting,
            pending_value,
            pointer_down,
            debounce_source,
            info_row,
            mute_button,
            default_button,
            port_rows,
        }
```

**Step 5: Build**

```bash
cargo build -p waft-settings 2>&1 | grep -E "^error"
```

Expected: no errors. There may be unused-variable warnings for the removed port combo fields — those are resolved in the next step.

**Step 6: Commit**

```bash
git add crates/settings/src/audio/device_card.rs
git commit -m "feat(settings/audio): replace port ComboRow with individual PortRow widgets"
```

---

### Task 4: Update reconciliation and remove old port helpers

**Files:**
- Modify: `crates/settings/src/audio/device_card.rs`

**Step 1: Update `reconcile_sinks()` — port update logic**

In `reconcile_sinks()`, find the port update block inside the "Update existing rows in place" branch (lines 358-369):

```rust
                // Port selection — guarded by `updating` flag ...
                *self.updating.borrow_mut() = true;
                Self::update_port_row(
                    &existing.port_row,
                    &existing.port_model,
                    &existing.port_names,
                    &sink.ports,
                    sink.active_port.as_deref(),
                );
                existing.port_row.set_visible(sink.ports.len() > 1);
                *self.updating.borrow_mut() = false;
```

Replace with in-place / rebuild logic. Also change `rows.iter().find()` to `rows.iter_mut().find()` since we now need a mutable reference for the rebuild case:

```rust
        // Update existing rows in place, create new rows for new sinks
        for sink in sinks {
            if let Some(existing) = rows.iter_mut().find(|r| r.sink_name == sink.sink_name) {
                // [keep all existing volume/mute/default update code unchanged]

                // Port rows — update in place if count unchanged, rebuild otherwise.
                if existing.port_rows.len() == sink.ports.len() {
                    for (pr, port) in existing.port_rows.iter().zip(sink.ports.iter()) {
                        pr.apply(port, sink.active_port.as_deref());
                    }
                } else {
                    for pr in &existing.port_rows {
                        existing.root.remove(pr.root.upcast_ref::<gtk::Widget>());
                    }
                    existing.port_rows = sink.ports.iter().map(|port| {
                        let sink_name = existing.sink_name.clone();
                        let pr = PortRow::new(
                            port,
                            sink.active_port.as_deref(),
                            move |port_name| AudioDeviceCardOutput::SetSinkPort {
                                sink: sink_name.clone(),
                                port: port_name,
                            },
                            &self.output_cb,
                        );
                        existing.root.append(pr.root.upcast_ref::<gtk::Widget>());
                        pr
                    }).collect();
                }
            } else {
```

The `*self.updating.borrow_mut()` guards around port updates are no longer needed and should be removed (the `updating` guard for the profile combo in `apply_props()` stays intact).

**Step 2: Update `reconcile_sources()` — port update logic**

Apply the same changes to `reconcile_sources()`:

- Change `rows.iter().find()` → `rows.iter_mut().find()`
- Replace the port combo update block with the same in-place/rebuild pattern, using `SetSourcePort` and `source_name` / `source_name` fields

**Step 3: Remove `init_port_row()` and `update_port_row()` helpers**

Delete the entire `init_port_row()` function (lines 1018-1037) and `update_port_row()` function (lines 1039-1059). They are no longer called.

**Step 4: Also remove the `sinks_box` / `sources_box` fields if they are now only used for adding children via `root.append`**

Wait — `sinks_box` and `sources_box` are `gtk::Box` containers added to the `PreferencesGroup`. They are still needed to hold the `SinkRow.root` (gtk::Box for info+slider). The port rows are inside the SinkRow.root already. So `sinks_box` and `sources_box` remain unchanged.

**Step 5: Build and test**

```bash
cargo build -p waft-settings 2>&1 | grep -E "^error"
cargo test --workspace 2>&1 | tail -20
```

Expected: clean build, all tests pass.

**Step 6: Commit**

```bash
git add crates/settings/src/audio/device_card.rs
git commit -m "refactor(settings/audio): update reconciliation for PortRow; remove port combo helpers"
```

---

### Task 5: Add device-type icon to card header

**Files:**
- Modify: `crates/settings/src/audio/device_card.rs`

**Step 1: Add header icon in `AudioDeviceCard::new()`**

In `new()`, after the `root` is built (line 175-177), add:

```rust
        // Device-type icon in the group header (right side via header_suffix).
        {
            let header_icon = IconWidget::from_name(
                audio_device_icon(&card.device_type, AudioDeviceKind::Output),
                24,
            );
            root.set_header_suffix(Some(header_icon.widget()));
            // GTK holds a ref to the widget; header_icon can drop here.
        }
```

Insert this block before the `// Profile combo row` comment.

**Step 2: Build and full test**

```bash
cargo build -p waft-settings 2>&1 | grep -E "^error"
cargo test --workspace 2>&1 | tail -20
```

Expected: clean build, all tests pass.

**Step 3: Commit**

```bash
git add crates/settings/src/audio/device_card.rs
git commit -m "feat(settings/audio): add device-type icon to card header suffix"
```

---

## Verification Checklist

After all tasks are complete, verify visually:

1. Each audio card has a device-type icon on the right of its group title
2. The `adw::PreferencesGroup` border (boxed-list style) is clearly visible around each card
3. All port rows are visible regardless of port count (no hidden ports)
4. Connected ports show "Connected" subtitle + normal icon; active port shows a checkmark
5. Disconnected ports show "Disconnected" subtitle, are dimmed, and cannot be clicked
6. Clicking a connected port selects it (verify via `pactl list sinks short` that the active port changed)
7. Profile combo still works
8. Volume slider debounce still works (drag slider rapidly, verify no visual yank)

## Smoke Test Commands

```bash
# Build and run settings app against a running daemon
WAFT_DAEMON_DIR=./target/debug cargo run --bin waft

# In another terminal:
WAFT_DAEMON_DIR=./target/debug cargo run --bin waft-settings
```
