# Brightness External Updates -- Implementation Plan

## Current State and Root Cause

The brightness plugin (`plugins/brightness/bin/waft-brightness-daemon.rs`) has **no external change monitoring at all**. It operates in a purely request-response pattern:

1. At startup, it discovers displays via `brightnessctl -l -m -c backlight` and `ddcutil detect --brief`, reading each display's current brightness.
2. When the user adjusts brightness through waft, `handle_action("set-brightness")` calls `brightnessctl set` or `ddcutil setvcp`, then updates local `displays: Mutex<Vec<Display>>` in-memory state.
3. The `main()` function ignores the `EntityNotifier` -- the closure signature is `|_notifier|`, discarding it. No background monitoring task is spawned.

When `swayosd-client` (or any external tool) changes brightness via `brightnessctl set` or writing directly to `/sys/class/backlight/*/brightness`, the plugin never learns about it. The stale value persists until plugin restart.

This contrasts with the audio plugin (spawns `pactl subscribe` + `notifier.notify()`) and the battery plugin (monitors UPower D-Bus `PropertiesChanged` signals).

---

## Technical Options

### Option A: inotify on sysfs backlight files (Recommended)

Linux backlight devices expose state at `/sys/class/backlight/{device}/actual_brightness` (read-only, reflects hardware state). These files emit inotify `MODIFY` events regardless of who made the change.

**Pros:**
- Zero latency, event-driven -- compliant with "NO POLLING" principle
- `notify` crate (v7) already used in the project (`plugins/xdg-apps/`)
- Established `spawn_blocking` + `std::sync::mpsc` pattern from xdg-apps can be reused
- Works regardless of which tool modified brightness

**Cons:**
- Only works for backlight devices, not DDC/CI external monitors
- Must watch `actual_brightness` (reflects hardware state, not requested value)

### Option B: Poll brightnessctl periodically

Run `brightnessctl -l -m` on a timer.

**Verdict:** Rejected -- violates "NO POLLING" design principle.

### Option C: udev events

Monitor udev for `backlight` subsystem changes.

**Verdict:** Over-engineered. Some drivers don't emit change events for brightness adjustments. Adds new dependency not in project.

### Option D: D-Bus logind brightness interface

systemd-logind has `SetBrightness` but no signal for brightness *changes*.

**Verdict:** Not viable -- no standard D-Bus interface emits brightness change signals.

### Option E: DDC/CI polling for external monitors

DDC/CI is slow I2C with no notification capability.

**Verdict:** Deferred. External monitor brightness changes by other tools are uncommon. Polling DDC/CI is expensive (100-500ms per query).

---

## Recommended Approach

**Use inotify (via `notify` crate) to watch sysfs backlight files.** Handles the primary use case (swayosd-client changing laptop backlight) with zero-latency, event-driven updates, following established project patterns.

---

## Implementation Steps

### Step 1: Add `notify` dependency

`plugins/brightness/Cargo.toml`:
```toml
notify = "7"
```

### Step 2: Restructure plugin state

Change to `Arc<StdMutex<Vec<Display>>>` for sharing with background watcher task (following audio plugin pattern):

```rust
struct BrightnessPlugin {
    displays: Arc<StdMutex<Vec<Display>>>,
}
```

### Step 3: Accept the notifier in `main()`

Change from `|_notifier|` to `|notifier|` in the `PluginRunner::run` closure.

### Step 4: Implement sysfs backlight watcher

Create `watch_backlight_brightness` function that:

1. Collects sysfs paths: `/sys/class/backlight/{device}/actual_brightness` for each backlight device
2. Creates a `notify::RecommendedWatcher` watching those files
3. Runs in `tokio::task::spawn_blocking` (matching xdg-apps pattern)
4. On `EventKind::Modify`, reads sysfs directly (`actual_brightness` / `max_brightness`) for the changed device
5. Updates `Arc<StdMutex<Vec<Display>>>` and calls `notifier.notify()`
6. Uses 50-100ms debounce to coalesce rapid changes during key-repeat

Reading sysfs directly is preferred over shelling out to `brightnessctl` on every event, since the watcher may fire rapidly during brightness key repeat.

### Step 5: Spawn the watcher in `main()`

```rust
fn main() -> Result<()> {
    PluginRunner::new("brightness", &[entity::display::DISPLAY_ENTITY_TYPE])
        .i18n(i18n(), "plugin-name", "plugin-description")
        .run(|notifier| async {
            let plugin = BrightnessPlugin::new().await?;
            let shared = plugin.shared_displays();

            let backlight_devices: Vec<String> = {
                let displays = shared.lock_or_recover();
                displays.iter()
                    .filter(|d| d.display_type == DisplayType::Backlight)
                    .map(|d| d.id.strip_prefix("backlight:").unwrap_or(&d.id).to_string())
                    .collect()
            };

            if !backlight_devices.is_empty() {
                spawn_monitored_anyhow("brightness-watcher",
                    watch_backlight_brightness(backlight_devices, shared, notifier));
            }

            Ok(plugin)
        })
}
```

### Step 6: Debounce

When holding a brightness key, many sysfs events fire rapidly. After receiving an event, wait 50-100ms for additional events before re-reading and notifying. This is sufficient to coalesce key-repeat without introducing perceptible lag.

---

## Questions Requiring User Input

1. **Sysfs vs brightnessctl for reading?** Direct sysfs reading (`actual_brightness`/`max_brightness`) is faster and avoids process spawning per event. But it couples to sysfs layout. Given that `brightnessctl` also reads sysfs and the interface is stable, direct reading is pragmatic. Confirm?

2. **DDC/CI monitors:** Accept that DDC/CI changes by external tools won't be detected for now? The TODO specifically mentions `swayosd-client` which is a backlight tool.

3. **Double notification on own changes:** The inotify watcher will see brightness changes made by waft itself. This is harmless (daemon deduplicates), but could be avoided by skipping events within a short window after an action. Worth the complexity?

4. **Debounce duration:** 50ms recommended. Configurable or hardcoded?

---

## Critical Files

- `plugins/brightness/bin/waft-brightness-daemon.rs` - All changes: state restructuring, watcher, notifier wiring
- `plugins/brightness/Cargo.toml` - Add `notify = "7"`
- `plugins/xdg-apps/bin/waft-xdg-apps-daemon.rs` - Reference pattern for `notify` crate usage
- `plugins/audio/bin/waft-audio-daemon.rs` - Reference pattern for `Arc<StdMutex<State>>` + background monitoring
- `crates/plugin/src/runner.rs` - `PluginRunner` and `spawn_monitored_anyhow` API reference
