# Calendar Sync Spinner

## Summary

Add a small spinner to the `EventsComponent` header in the overview that appears whenever
the EDS plugin is actively performing a calendar refresh — regardless of what triggered it
(overlay show, periodic background timer, or session unlock).

## Files changed

| File | Change |
|------|--------|
| `crates/protocol/src/entity/calendar.rs` | Add `syncing: bool` to `CalendarSync` |
| `plugins/eds/bin/waft-eds-daemon.rs` | Add syncing state, notifier slot, `refresh_with_status`, update all call sites |
| `crates/overview/src/components/events.rs` | Subscribe to CalendarSync, show/hide SpinnerWidget |

---

## Layer 1 — Protocol

Add one field to `CalendarSync`:

```rust
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct CalendarSync {
    pub last_refresh: Option<i64>,
    pub syncing: bool,   // true while a refresh is in progress
}
```

Default is `false`. The plugin is the only writer; the overview only reads.

---

## Layer 2 — EDS plugin

### EdsState

Add `syncing: bool` (default `false`) alongside the existing fields.

### EdsPlugin — notifier slot

`handle_action` is a Plugin trait method with no access to the EntityNotifier. Bridge this
with a shared slot:

```rust
struct EdsPlugin {
    // ... existing fields ...
    notifier: Arc<std::sync::Mutex<Option<EntityNotifier>>>,
}
```

In `main()`, after `PluginRuntime::new("eds", plugin)` returns the notifier, fill the slot:

```rust
*plugin_notifier.lock().unwrap() = Some(notifier.clone());
```

(`plugin_notifier` is the `Arc` cloned before the move.)

### `refresh_with_status` helper

Replace direct `do_refresh` calls with a wrapper that brackets the operation with state
transitions and notifications:

```rust
async fn refresh_with_status(
    conn: &Connection,
    state: &Arc<StdMutex<EdsState>>,
    notifier: &EntityNotifier,
    backends: &[(String, String)],
) {
    // Mark syncing start
    {
        let mut st = state.lock() /* poison recovery */;
        st.syncing = true;
    }
    notifier.notify();

    do_refresh(conn, backends).await;

    // Mark syncing end + record timestamp
    {
        let mut st = state.lock() /* poison recovery */;
        st.syncing = false;
        st.last_refresh = Some(unix_now());
    }
    notifier.notify();
}
```

`unix_now()` is a small inline helper returning `SystemTime::now()` as `i64` seconds.

### Call sites updated

| Location | Before | After |
|----------|--------|-------|
| `handle_action("refresh")` | `do_refresh(&self.conn, &backends).await` | `refresh_with_status(&self.conn, &self.state, &notifier, &backends).await` |
| `spawn_refresh_scheduler` — timer arm | `do_refresh(&conn, &backends).await` | `refresh_with_status(&conn, &state, &notifier, &backends).await` |
| `spawn_refresh_scheduler` — unlock arm | `do_refresh(&conn, &backends).await` | `refresh_with_status(&conn, &state, &notifier, &backends).await` |

`spawn_refresh_scheduler` receives an additional `notifier: EntityNotifier` parameter.
`main()` passes `notifier.clone()` to it.

`last_refresh` is now set inside `refresh_with_status`, so the separate `last_refresh`
update in `handle_action` is removed.

---

## Layer 3 — Overview

### `EventsComponent`

The component already receives an `EntityStore` reference. Add a `CalendarSync`
subscription in the constructor alongside the existing subscriptions:

1. Create a `SpinnerWidget::new(false)` and pack it into the header `gtk::Box` **to the
   left of the toggle buttons**.
2. Subscribe to `entity::calendar::CALENDAR_SYNC_ENTITY_TYPE`.
3. On every change, read the latest `CalendarSync` entity, call:
   - `spinner.set_spinning(syncing)`
   - `spinner.set_visible(syncing)` — collapses the slot when idle, avoiding a blank gap
4. Add `idle_add_local_once` initial reconciliation (per the EntityStore pattern) so the
   spinner reflects state already cached before the subscription was registered.

No new files. The spinner lives entirely in `events.rs`.

---

## Spinner sizing

Use pixel size **16** (matches the existing toggle button icons in the header row).

---

## Failure / edge cases

| Scenario | Behaviour |
|----------|-----------|
| EDS plugin not running | `CalendarSync` entity absent → spinner stays hidden |
| Refresh debounced | `handle_action` returns early without calling `refresh_with_status` → `syncing` never set → spinner stays hidden |
| Refresh fails mid-D-Bus | `do_refresh` logs the error; `refresh_with_status` still clears `syncing` in the finally-equivalent block → spinner always hides |
| Overlay hidden during refresh | Spinner state is irrelevant; next show will reflect current entity state |
