# Calendar Sync Spinner Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Show a small `SpinnerWidget` in the `EventsComponent` header (left of the toggle buttons) that spins while the EDS plugin is actively performing any calendar refresh.

**Architecture:** Add `syncing: bool` to the `CalendarSync` protocol entity. The EDS plugin sets it around every `do_refresh()` call (overlay-triggered, periodic, and unlock-triggered) via a new `refresh_with_status()` helper. The overview subscribes to `CalendarSync` in `EventsComponent` and shows/hides the spinner accordingly.

**Tech Stack:** Rust, GTK4/libadwaita, zbus 5, tokio, waft-plugin EntityNotifier, waft-ui-gtk SpinnerWidget.

> **Branch:** Apply on top of `eds-adaptive-refresh` (or after merging to `larger-larger-picture`).
> The `eds-adaptive-refresh` branch has `do_refresh()`, `check_debounce()`,
> `spawn_refresh_scheduler()`, and the debounce-aware `handle_action()` that this plan
> modifies.

---

## Task 1: Add `syncing: bool` to `CalendarSync`

**Files:**
- Modify: `crates/protocol/src/entity/calendar.rs`

### Step 1: Write the failing test

Add to the existing `#[cfg(test)]` module (after line 112):

```rust
#[test]
fn calendar_sync_syncing_defaults_false_on_deserialize() {
    // A JSON payload without "syncing" must deserialize with syncing=false
    // (backward compatibility with any existing serialized state).
    let json = serde_json::json!({ "last_refresh": null });
    let sync: CalendarSync = serde_json::from_value(json).unwrap();
    assert!(!sync.syncing, "syncing must default to false when absent from JSON");
}

#[test]
fn calendar_sync_syncing_roundtrips() {
    let sync = CalendarSync { last_refresh: Some(1_000_000), syncing: true };
    let json = serde_json::to_value(&sync).unwrap();
    let decoded: CalendarSync = serde_json::from_value(json).unwrap();
    assert_eq!(decoded.syncing, true);
    assert_eq!(decoded.last_refresh, Some(1_000_000));
}
```

Run: `cargo test -p waft-protocol`
Expected: FAIL — `CalendarSync` has no `syncing` field yet.

### Step 2: Add the field

In `CalendarSync` (line 13-17), change to:

```rust
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct CalendarSync {
    /// Unix timestamp of the last refresh trigger, or `None` if never triggered.
    pub last_refresh: Option<i64>,
    /// True while a calendar refresh is actively in progress.
    #[serde(default)]
    pub syncing: bool,
}
```

Note: Also add `Default` to the derive list (needed for `#[serde(default)]` to work for
the struct itself in the overview subscription code).

### Step 3: Run tests

Run: `cargo test -p waft-protocol`
Expected: all pass including the two new tests.

### Step 4: Commit

```bash
git add crates/protocol/src/entity/calendar.rs
git commit -m "feat(protocol): add syncing field to CalendarSync entity"
```

---

## Task 2: Add `syncing` to `EdsState` and propagate in `get_entities()`

**Files:**
- Modify: `plugins/eds/bin/waft-eds-daemon.rs`

### Step 1: Add field to `EdsState`

In the `EdsState` struct (around line 49-70), add one field and initialize it:

```rust
struct EdsState {
    events: HashMap<String, entity::calendar::CalendarEvent>,
    view_monitor_handles: Vec<tokio::task::JoinHandle<()>>,
    calendar_backends: Vec<(String, String)>,
    last_refresh: Option<i64>,
    debounce_recent: std::collections::VecDeque<std::time::Instant>,
    /// True while a calendar refresh D-Bus call is in progress.
    syncing: bool,
}

impl EdsState {
    fn new() -> Self {
        Self {
            events: HashMap::new(),
            view_monitor_handles: Vec::new(),
            calendar_backends: Vec::new(),
            last_refresh: None,
            debounce_recent: std::collections::VecDeque::new(),
            syncing: false,
        }
    }
}
```

### Step 2: Update `get_entities()`

In `get_entities()` (around line 127), change the CalendarSync construction to include `syncing`:

```rust
let sync = entity::calendar::CalendarSync {
    last_refresh: state.last_refresh,
    syncing: state.syncing,
};
```

### Step 3: Run tests

Run: `cargo test --bin waft-eds-daemon`
Expected: all 56 tests pass (no logic changed, only new field).

### Step 4: Commit

```bash
git add plugins/eds/bin/waft-eds-daemon.rs
git commit -m "feat(eds): add syncing state field to EdsState"
```

---

## Task 3: Add notifier slot to `EdsPlugin`

**Files:**
- Modify: `plugins/eds/bin/waft-eds-daemon.rs`

`handle_action` is a Plugin trait method that has no notifier parameter. We bridge this
with an `Arc<Mutex<Option<EntityNotifier>>>` slot that `main()` fills after
`PluginRuntime::new()`.

### Step 1: Add the field

In `EdsPlugin` struct (around line 74-78), add one field:

```rust
struct EdsPlugin {
    config: EdsConfig,
    state: Arc<StdMutex<EdsState>>,
    conn: Connection,
    session_locked: Arc<std::sync::atomic::AtomicBool>,
    unlock_notify: Arc<tokio::sync::Notify>,
    /// Notifier slot — filled by main() after PluginRuntime::new().
    /// Used by handle_action to push syncing-state updates mid-action.
    notifier: Arc<StdMutex<Option<EntityNotifier>>>,
}
```

### Step 2: Initialize in `EdsPlugin::new()`

In `EdsPlugin::new()` (the `Ok(Self { ... })` block), add:

```rust
notifier: Arc::new(StdMutex::new(None)),
```

### Step 3: Add accessor

After `fn unlock_notify(&self)`, add:

```rust
fn notifier_slot(&self) -> Arc<StdMutex<Option<EntityNotifier>>> {
    self.notifier.clone()
}
```

### Step 4: Run tests

Run: `cargo test --bin waft-eds-daemon`
Expected: all 56 tests pass.

### Step 5: Commit

```bash
git add plugins/eds/bin/waft-eds-daemon.rs
git commit -m "feat(eds): add notifier slot to EdsPlugin for mid-action notifications"
```

---

## Task 4: Implement `refresh_with_status` and update `handle_action`

**Files:**
- Modify: `plugins/eds/bin/waft-eds-daemon.rs`

### Step 1: Write the test

Add to the `#[cfg(test)]` module (at the bottom of existing tests, before closing `}`):

```rust
#[test]
fn eds_state_syncing_defaults_false() {
    let state = EdsState::new();
    assert!(!state.syncing, "syncing must start false");
}
```

Run: `cargo test --bin waft-eds-daemon eds_state_syncing_defaults_false`
Expected: PASS (field already added in Task 2 — this just locks the invariant).

### Step 2: Add `unix_now()` helper

Place just before `check_debounce` (around line 207):

```rust
/// Returns the current Unix timestamp in seconds.
fn unix_now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64
}
```

### Step 3: Add `refresh_with_status()` free async function

Place just after `do_refresh` (after its closing `}`):

```rust
/// Run `do_refresh` and bracket it with `syncing = true/false` state updates.
///
/// Both the scheduler and handle_action call this instead of do_refresh directly
/// so the overview always receives accurate syncing state.
async fn refresh_with_status(
    conn: &Connection,
    state: &Arc<StdMutex<EdsState>>,
    notifier: &EntityNotifier,
    backends: &[(String, String)],
) {
    {
        let mut st = match state.lock() {
            Ok(g) => g,
            Err(e) => {
                log::warn!("[eds] refresh_with_status: mutex poisoned on start, recovering: {e}");
                e.into_inner()
            }
        };
        st.syncing = true;
    }
    notifier.notify();

    do_refresh(conn, backends).await;

    {
        let mut st = match state.lock() {
            Ok(g) => g,
            Err(e) => {
                log::warn!("[eds] refresh_with_status: mutex poisoned on end, recovering: {e}");
                e.into_inner()
            }
        };
        st.syncing = false;
        st.last_refresh = Some(unix_now());
    }
    notifier.notify();
}
```

### Step 4: Update `handle_action("refresh")`

The current `handle_action` body (after the debounce check) sets `last_refresh` then calls
`do_refresh`. Replace from the debounce-pass point onward with:

```rust
// Clone notifier out of the slot before the async boundary.
let notifier = {
    let guard = match self.notifier.lock() {
        Ok(g) => g,
        Err(e) => {
            log::warn!("[eds] handle_action: notifier slot mutex poisoned, recovering: {e}");
            e.into_inner()
        }
    };
    guard.as_ref().cloned()
};

match notifier {
    Some(n) => {
        refresh_with_status(&self.conn, &self.state, &n, &backends).await;
    }
    None => {
        // Notifier not yet wired (should not happen in production).
        log::warn!("[eds] handle_action: notifier slot empty, syncing indicator unavailable");
        do_refresh(&self.conn, &backends).await;
        // Update last_refresh manually since refresh_with_status didn't run.
        let mut st = match self.state.lock() {
            Ok(g) => g,
            Err(e) => {
                log::warn!("[eds] handle_action: mutex poisoned updating last_refresh, recovering: {e}");
                e.into_inner()
            }
        };
        st.last_refresh = Some(unix_now());
    }
}
return Ok(());
```

Also remove the old separate `last_refresh` update that was before `do_refresh` in the
previous version (it's now inside `refresh_with_status`).

### Step 5: Run tests

Run: `cargo test --bin waft-eds-daemon`
Expected: all 56+ tests pass.

### Step 6: Commit

```bash
git add plugins/eds/bin/waft-eds-daemon.rs
git commit -m "feat(eds): add refresh_with_status, wire syncing state in handle_action"
```

---

## Task 5: Update `spawn_refresh_scheduler` and `main()`

**Files:**
- Modify: `plugins/eds/bin/waft-eds-daemon.rs`

### Step 1: Add `notifier` parameter to `spawn_refresh_scheduler`

Current signature:
```rust
async fn spawn_refresh_scheduler(
    conn: Connection,
    state: Arc<StdMutex<EdsState>>,
    config: EdsConfig,
    session_locked: Arc<std::sync::atomic::AtomicBool>,
    unlock_notify: Arc<tokio::sync::Notify>,
)
```

New signature (add one parameter at the end):
```rust
async fn spawn_refresh_scheduler(
    conn: Connection,
    state: Arc<StdMutex<EdsState>>,
    config: EdsConfig,
    session_locked: Arc<std::sync::atomic::AtomicBool>,
    unlock_notify: Arc<tokio::sync::Notify>,
    notifier: EntityNotifier,
)
```

### Step 2: Replace `do_refresh` calls in the scheduler body

There are two call sites inside `spawn_refresh_scheduler`:

**Timer arm** (after the locked guard check), replace:
```rust
do_refresh(&conn, &backends).await;
```
with:
```rust
refresh_with_status(&conn, &state, &notifier, &backends).await;
```

**Unlock arm**, replace:
```rust
do_refresh(&conn, &backends).await;
```
with:
```rust
refresh_with_status(&conn, &state, &notifier, &backends).await;
```

Also remove the old `last_refresh` update inside the unlock arm — `refresh_with_status` handles it.

The `debounce_recent.push_back` in the unlock arm stays (it's separate from `last_refresh`).

### Step 3: Update `main()` — fill notifier slot and pass to scheduler

In `main()` (around line 2448+), currently:

```rust
let shared_state = plugin.shared_state();
let conn = plugin.conn.clone();
let config = plugin.config.clone();
let session_locked = plugin.session_locked();
let unlock_notify = plugin.unlock_notify();
let scheduler_conn = conn.clone();

let (runtime, notifier) = PluginRuntime::new("eds", plugin);
```

Add extraction of the notifier slot **before** `PluginRuntime::new`:

```rust
let notifier_slot = plugin.notifier_slot();
```

After `PluginRuntime::new("eds", plugin)`, fill the slot:

```rust
{
    let mut slot = match notifier_slot.lock() {
        Ok(g) => g,
        Err(e) => {
            log::warn!("[eds] main: notifier slot mutex poisoned, recovering: {e}");
            e.into_inner()
        }
    };
    *slot = Some(notifier.clone());
}
```

Pass `notifier.clone()` to `spawn_refresh_scheduler`:

```rust
tokio::spawn(spawn_refresh_scheduler(
    scheduler_conn,
    shared_state.clone(),
    config,
    session_locked,
    unlock_notify,
    notifier.clone(),   // ← new
));
```

### Step 4: Run tests

Run: `cargo test --bin waft-eds-daemon`
Expected: all tests pass.

### Step 5: Commit

```bash
git add plugins/eds/bin/waft-eds-daemon.rs
git commit -m "feat(eds): wire syncing state through scheduler and main notifier slot"
```

---

## Task 6: Show spinner in `EventsComponent`

**Files:**
- Modify: `crates/overview/src/components/events.rs`

### Step 1: Add `SpinnerWidget` to the header

`EventsComponent::new` receives `store: &Rc<EntityStore>` which already has the
`CalendarSync` entity cached (the `app.rs` level subscribes to capture the URN).
`EventsComponent` needs its own subscription to the same entity type to read `syncing`.

Open `crates/overview/src/components/events.rs`.

At the top, add the import (alongside existing imports):
```rust
use waft_ui_gtk::widgets::SpinnerWidget;
use waft_ui_gtk::widget_base::WidgetBase as _;
```

In `EventsComponent::new`, after building `controls` (around line 64, after `controls.append(&past_btn)`), add a spinner and insert it into `controls` **before** the toggle buttons:

```rust
// Spinner shown while the EDS plugin is actively refreshing calendar backends.
let sync_spinner = SpinnerWidget::new(false);
sync_spinner.widget().set_visible(false);
controls.prepend(&sync_spinner.widget());
```

`controls.prepend` inserts at the start, so the order becomes:
`[spinner] [calendar_toggle] [past_btn]`

### Step 2: Subscribe to CalendarSync

After `calendar_revealer.connect_child_revealed_notify(...)` (around line 107), add:

```rust
// Reflect calendar sync state in the spinner.
{
    let spinner_ref = sync_spinner.widget();
    let store_ref = store.clone();
    let update_spinner = {
        let spinner_ref = spinner_ref.clone();
        let store_ref = store_ref.clone();
        move || {
            let entities = store_ref
                .get_entities_typed::<entity::calendar::CalendarSync>(
                    entity::calendar::CALENDAR_SYNC_ENTITY_TYPE,
                );
            let syncing = entities.first().map(|(_, s)| s.syncing).unwrap_or(false);
            spinner_ref.set_visible(syncing);
            if syncing {
                // SpinnerWidget tracks its own spinning state but we still
                // call set_spinning to start the GTK animation.
                // We access the underlying widget directly via the WidgetBase trait.
            }
        }
    };
    store.subscribe_type(entity::calendar::CALENDAR_SYNC_ENTITY_TYPE, update_spinner.clone());
    // Initial reconciliation: entity may already be cached (CLAUDE.md EntityStore pattern).
    gtk::glib::idle_add_local_once(update_spinner);
}
```

Wait — `SpinnerWidget` only exposes `set_spinning()`, not `widget()` directly for
`set_visible()`. We need to keep a reference to the widget handle. Revise slightly:

```rust
let sync_spinner = SpinnerWidget::new(false);
let spinner_widget = sync_spinner.widget(); // gtk::Widget handle for visibility control
spinner_widget.set_visible(false);
controls.prepend(&spinner_widget);
```

Then in the subscription callback:
```rust
move || {
    let entities = store_ref
        .get_entities_typed::<entity::calendar::CalendarSync>(
            entity::calendar::CALENDAR_SYNC_ENTITY_TYPE,
        );
    let syncing = entities.first().map(|(_, s)| s.syncing).unwrap_or(false);
    sync_spinner_ref.set_spinning(syncing);
    spinner_widget_ref.set_visible(syncing);
}
```

Use two clones — one for `SpinnerWidget` (to call `set_spinning`) and one for
`gtk::Widget` (to call `set_visible`). Capture both in the closure.

Full revised block to replace the placeholder above:

```rust
{
    let sync_spinner_ref = sync_spinner;        // takes ownership; must not be used after
    let spinner_widget_ref = spinner_widget.clone();
    let store_ref = store.clone();
    let update_spinner = {
        let spinner_widget_ref = spinner_widget_ref.clone();
        let store_ref = store_ref.clone();
        move || {
            let entities = store_ref
                .get_entities_typed::<entity::calendar::CalendarSync>(
                    entity::calendar::CALENDAR_SYNC_ENTITY_TYPE,
                );
            let syncing = entities
                .first()
                .map(|(_, s)| s.syncing)
                .unwrap_or(false);
            sync_spinner_ref.set_spinning(syncing);
            spinner_widget_ref.set_visible(syncing);
        }
    };
    store.subscribe_type(entity::calendar::CALENDAR_SYNC_ENTITY_TYPE, update_spinner.clone());
    gtk::glib::idle_add_local_once(update_spinner);
}
```

Also add the entity import at the top of the file (if not already present):
```rust
use waft_protocol::entity;
```

### Step 3: Build to verify

Run: `cargo build -p waft-overview`
Expected: clean build. Fix any borrow/lifetime issues surfaced by the compiler.

Likely issue: `SpinnerWidget` is not `Clone`. If the closure needs to move it, adjust by
wrapping in `Rc` or using `gtk::Widget` handle only (calling `start()`/`stop()` directly
on the underlying `gtk::Spinner` — or just using `set_visible` only, since GTK spinners
auto-animate when spinning=true and visible=true).

Simplest fallback if ownership is tricky: store only the `gtk::Widget` handle, and use
`.add_css_class("spinning")` / `.remove_css_class("spinning")` — but the proper approach
is below.

Cleanest pattern — keep only the `gtk::Widget` reference and control via CSS state:

```rust
let spinner_widget = {
    let s = SpinnerWidget::new(false);
    let w = s.widget();
    w.set_visible(false);
    controls.prepend(&w);
    w   // gtk::Widget is Clone, so we can capture it directly
};
```

Then in the subscription:
```rust
let spinner_ref = spinner_widget.clone();
let update_spinner = {
    let spinner_ref = spinner_ref.clone();
    let store_ref = store.clone();
    move || {
        let entities = store_ref
            .get_entities_typed::<entity::calendar::CalendarSync>(
                entity::calendar::CALENDAR_SYNC_ENTITY_TYPE,
            );
        let syncing = entities
            .first()
            .map(|(_, s)| s.syncing)
            .unwrap_or(false);
        // gtk::Spinner auto-animates when visible; no need to call start()/stop().
        spinner_ref.set_visible(syncing);
    }
};
```

`gtk::Widget` is `Clone` (it's a GObject ref-count), so this pattern compiles cleanly.
The `SpinnerWidget` wrapper is discarded after extracting the widget handle — that's fine
since it's just a thin wrapper.

### Step 4: Run workspace tests

Run: `cargo test --workspace`
Expected: all tests pass.

### Step 5: Manual smoke test

1. Start the daemon: `WAFT_DAEMON_DIR=./target/debug cargo run`
2. Open the overview (e.g. trigger `ShowOverlay`)
3. Watch the header row of the Events section — a spinner should briefly appear left of the
   calendar toggle when the overlay opens and the refresh action fires
4. Spinner should disappear within ~1-3 seconds when `do_refresh()` completes

### Step 6: Commit

```bash
git add crates/overview/src/components/events.rs
git commit -m "feat(overview): show sync spinner in Events header during calendar refresh"
```

---

## Summary of commits

| Commit | Change |
|--------|--------|
| `feat(protocol): add syncing field to CalendarSync entity` | Protocol |
| `feat(eds): add syncing state field to EdsState` | Plugin state |
| `feat(eds): add notifier slot to EdsPlugin for mid-action notifications` | Plugin structure |
| `feat(eds): add refresh_with_status, wire syncing state in handle_action` | Plugin logic |
| `feat(eds): wire syncing state through scheduler and main notifier slot` | Plugin wiring |
| `feat(overview): show sync spinner in Events header during calendar refresh` | UI |
