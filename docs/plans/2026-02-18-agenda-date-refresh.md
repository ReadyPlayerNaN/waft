# Agenda Date Refresh Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Fix the agenda widget showing stale dates by adding sleep-to-deadline refresh timers in both the widget and the EDS plugin.

**Architecture:** The agenda widget arms a one-shot glib timer after every `update_events()` call, sleeping until the next visible display change (earliest future event end or midnight). The EDS plugin spawns a tokio midnight-loop that aborts old view monitors, purges stale events, and rebuilds CalendarViews anchored to the new day.

**Tech Stack:** Rust, GTK4 (glib::timeout_add_seconds_local), tokio::time::sleep, chrono::Local

---

## Task 1: Widget helper functions + unit tests

**Files:**
- Modify: `crates/overview/src/components/agenda.rs` (after line 460, before the `#[cfg(test)]` block if one exists, or at end of file)

**Step 1: Write the failing tests**

Add at the bottom of `agenda.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use waft_protocol::{Urn, entity};

    fn make_event(end_offset_secs: i64) -> (Urn, entity::calendar::CalendarEvent) {
        let now = chrono::Local::now().timestamp();
        let event = entity::calendar::CalendarEvent {
            uid: "test-uid".to_string(),
            summary: "Test event".to_string(),
            start_time: now - 3600,
            end_time: now + end_offset_secs,
            all_day: false,
            description: None,
            location: None,
            attendees: vec![],
        };
        let urn = Urn::new("eds", entity::calendar::ENTITY_TYPE, "test-uid@0");
        (urn, event)
    }

    #[test]
    fn secs_until_next_midnight_is_positive() {
        let secs = secs_until_next_midnight();
        assert!(secs > 0, "must be at least 1 second");
        assert!(secs <= 86401, "must be at most one day + 1 second clamp");
    }

    #[test]
    fn next_boundary_no_events_returns_midnight() {
        let result = next_boundary_secs(&[]);
        let midnight = secs_until_next_midnight();
        assert_eq!(result, midnight);
    }

    #[test]
    fn next_boundary_event_before_midnight_returns_event_end() {
        // Event ending in 5 minutes
        let (urn, event) = make_event(300);
        let result = next_boundary_secs(&[(urn, event)]);
        assert!(result <= 300, "should be at most 300s, got {result}");
        assert!(result > 0, "must be at least 1 second");
    }

    #[test]
    fn next_boundary_event_after_midnight_returns_midnight() {
        // Event ending 26 hours from now (past tomorrow midnight)
        let (urn, event) = make_event(26 * 3600);
        let result = next_boundary_secs(&[(urn, event)]);
        let midnight = secs_until_next_midnight();
        assert_eq!(result, midnight, "should sleep to midnight, not past it");
    }

    #[test]
    fn next_boundary_picks_earliest_future_event() {
        let event_far = make_event(26 * 3600);
        let event_near = make_event(120); // 2 minutes
        let result = next_boundary_secs(&[event_far, event_near]);
        assert!(result <= 120);
        assert!(result > 0);
    }
}
```

**Step 2: Run tests to confirm they fail**

```bash
cargo test -p waft-overview secs_until_next_midnight 2>&1 | tail -5
cargo test -p waft-overview next_boundary 2>&1 | tail -5
```

Expected: errors like `cannot find function 'secs_until_next_midnight'`.

**Step 3: Implement the helpers**

Add these two functions just before the `impl AgendaComponent` block (around line 57):

```rust
/// Returns seconds until local midnight (minimum 1).
fn secs_until_next_midnight() -> u64 {
    let now = chrono::Local::now();
    let tomorrow = (now.date_naive() + chrono::Duration::days(1))
        .and_hms_opt(0, 0, 0)
        .expect("midnight is always valid")
        .and_local_timezone(chrono::Local)
        .earliest()
        .expect("tomorrow midnight is a valid local time");
    (tomorrow.timestamp() - now.timestamp()).max(1) as u64
}

/// Returns seconds until the next display-relevant change: whichever comes first —
/// an in-window future event ending, or local midnight.
fn next_boundary_secs(future_events: &[(Urn, entity::calendar::CalendarEvent)]) -> u64 {
    let now = chrono::Local::now().timestamp();
    let midnight = secs_until_next_midnight();
    future_events
        .iter()
        .map(|(_, e)| e.end_time)
        .filter(|&t| t > now)
        .min()
        .map(|t| ((t - now).max(1) as u64).min(midnight))
        .unwrap_or(midnight)
}
```

**Step 4: Run tests to confirm they pass**

```bash
cargo test -p waft-overview secs_until_next_midnight next_boundary 2>&1 | tail -10
```

Expected: all 5 tests pass.

**Step 5: Commit**

```bash
git add crates/overview/src/components/agenda.rs
git commit -m "feat(agenda): add secs_until_next_midnight and next_boundary_secs helpers"
```

---

## Task 2: Wire sleep-to-deadline timer into the rebuild cycle

**Files:**
- Modify: `crates/overview/src/components/agenda.rs`

The `rebuild` closure is a `Rc<dyn Fn()>` built at lines 172–195 of `agenda.rs`. The timer callback needs to call `rebuild()` again, but `rebuild` captures itself — which is a circular reference. The standard GTK Rust pattern is an indirection cell whose weak pointer breaks the cycle.

**Step 1: Add timer state before the `rebuild` closure (around line 171)**

Insert before `let rebuild = {`:

```rust
// Timer source — stores the pending SourceId so we can cancel it before re-arming.
let timer_source: Rc<RefCell<Option<glib::SourceId>>> = Rc::new(RefCell::new(None));

// Holder that lets the timer callback call back into `rebuild` without a strong cycle.
// `rebuild` holds a Weak to this; this holds a Strong to `rebuild`.
let rebuild_holder: Rc<RefCell<Option<Rc<dyn Fn()>>>> = Rc::new(RefCell::new(None));
```

**Step 2: Extend the `rebuild` closure to arm the next timer**

The existing closure (lines 182–194) looks like:

```rust
Rc::new(move || {
    let selected_date = selection_store_ref.get_state().selected_date;
    Self::update_events(
        &store_ref,
        &event_cards_ref,
        &past_box_ref,
        &content_box_ref,
        &empty_label_ref,
        &menu_store_ref,
        &now_divider,
        selected_date,
    );
})
```

Add captures for `store_ref_timer`, `timer_source_ref`, and `rebuild_holder_weak` **inside** the `let rebuild = {` block alongside the existing captures, then extend the closure body:

```rust
let rebuild = {
    // existing captures …
    let store_ref = store.clone();
    let event_cards_ref = event_cards.clone();
    let past_box_ref = past_box.clone();
    let content_box_ref = content_box.clone();
    let empty_label_ref = empty_label.clone();
    let menu_store_ref = menu_store.clone();
    let selection_store_ref = selection_store.clone();
    let now_divider = Rc::new(RefCell::new(None::<gtk::Separator>));

    // NEW: timer captures
    let store_ref_timer = store.clone();
    let timer_source_ref = timer_source.clone();
    let rebuild_holder_weak = Rc::downgrade(&rebuild_holder);

    Rc::new(move || {
        let selected_date = selection_store_ref.get_state().selected_date;
        Self::update_events(
            &store_ref,
            &event_cards_ref,
            &past_box_ref,
            &content_box_ref,
            &empty_label_ref,
            &menu_store_ref,
            &now_divider,
            selected_date,
        );

        // --- sleep-to-deadline timer ---
        // Compute future events to find next boundary.
        let now = chrono::Local::now().timestamp();
        let all: Vec<(Urn, entity::calendar::CalendarEvent)> =
            store_ref_timer.get_entities_typed(entity::calendar::ENTITY_TYPE);
        let future_events: Vec<_> = all.into_iter().filter(|(_, e)| e.end_time > now).collect();
        let secs = next_boundary_secs(&future_events);

        // Cancel the previous timer, arm a new one-shot timer.
        let mut timer = timer_source_ref.borrow_mut();
        if let Some(id) = timer.take() {
            id.remove();
        }
        let holder_weak = rebuild_holder_weak.clone();
        let ts = timer_source_ref.clone();
        *timer = Some(glib::timeout_add_seconds_local(secs, move || {
            ts.borrow_mut().take(); // clear our own SourceId
            if let Some(holder) = holder_weak.upgrade() {
                if let Some(rebuild_fn) = &*holder.borrow() {
                    rebuild_fn();
                }
            }
            glib::ControlFlow::Break
        }));
    }) as Rc<dyn Fn()>
};

// Connect holder → rebuild (after the closure is built).
*rebuild_holder.borrow_mut() = Some(rebuild.clone());
```

**Step 3: Keep the holder alive in `AgendaComponent`**

Add two fields to the `AgendaComponent` struct (around line 36):

```rust
pub struct AgendaComponent {
    // ... existing fields ...
    _refresh_holder: Rc<RefCell<Option<Rc<dyn Fn()>>>>,
    _timer_source: Rc<RefCell<Option<glib::SourceId>>>,
}
```

And populate them in the `Self { ... }` construction (around line 209):

```rust
Self {
    // ... existing fields ...
    _refresh_holder: rebuild_holder,
    _timer_source: timer_source,
}
```

**Step 4: Fire an initial rebuild via idle (arms the first timer)**

After the two `store.subscribe_type` / `selection_store.subscribe` calls (around line 207), add:

```rust
// Arm the first sleep-to-deadline timer via idle so all subscriptions
// are registered before we read entity state.
let rebuild_init = rebuild.clone();
gtk::glib::idle_add_local_once(move || {
    rebuild_init();
});
```

**Step 5: Build to check for compile errors**

```bash
cargo build -p waft-overview 2>&1 | grep -E "^error"
```

Expected: no errors.

**Step 6: Run all overview tests**

```bash
cargo test -p waft-overview 2>&1 | tail -15
```

Expected: all tests pass.

**Step 7: Commit**

```bash
git add crates/overview/src/components/agenda.rs
git commit -m "feat(agenda): sleep-to-deadline timer refreshes date labels and past/future split"
```

---

## Task 3: Add `JoinHandle` tracking to `EdsState`

**Files:**
- Modify: `plugins/eds/bin/waft-eds-daemon.rs`

**Step 1: Add the field to `EdsState` (around line 30)**

```rust
struct EdsState {
    /// Map of occurrence keys to calendar events.
    /// Key format: "{uid}@{start_time}"
    events: HashMap<String, entity::calendar::CalendarEvent>,
    /// Handles for running view-monitor tasks. Aborted on midnight rebuild.
    view_monitor_handles: Vec<tokio::task::JoinHandle<()>>,
}

impl EdsState {
    fn new() -> Self {
        Self {
            events: HashMap::new(),
            view_monitor_handles: Vec::new(),
        }
    }
}
```

**Step 2: Build to verify**

```bash
cargo build -p waft-eds-daemon 2>&1 | grep -E "^error"
```

Expected: no errors.

**Step 3: Commit**

```bash
git add plugins/eds/bin/waft-eds-daemon.rs
git commit -m "feat(eds): add view_monitor_handles to EdsState for midnight rebuild"
```

---

## Task 4: Extract `setup_calendar_views` helper

**Files:**
- Modify: `plugins/eds/bin/waft-eds-daemon.rs`

The goal is to extract the discovery + view-creation loop from `monitor_eds_calendars` (lines 302–389) into a standalone async function that can be called both at startup and by the midnight loop.

**Step 1: Add a secs-to-midnight helper at the plugin level**

Add just before `monitor_eds_calendars`:

```rust
/// Returns seconds until the next local midnight (minimum 1).
fn secs_until_eds_midnight() -> u64 {
    let now = chrono::Local::now();
    let tomorrow = (now.date_naive() + chrono::Duration::days(1))
        .and_hms_opt(0, 0, 0)
        .expect("midnight is always valid")
        .and_local_timezone(chrono::Local)
        .earliest()
        .expect("tomorrow midnight is a valid local time");
    (tomorrow.timestamp() - now.timestamp()).max(1) as u64
}
```

**Step 2: Extract `setup_calendar_views`**

Add a new async function that performs discovery + view setup and returns `JoinHandle`s:

```rust
/// Discover EDS calendar sources, create views for today→+30d, and spawn
/// view-monitor tasks. Returns the handles so callers can abort them later.
async fn setup_calendar_views(
    conn: &Connection,
    state: Arc<StdMutex<EdsState>>,
    notifier: EntityNotifier,
) -> Vec<tokio::task::JoinHandle<()>> {
    let sources = match discover_calendar_sources(conn).await {
        Ok(s) => s,
        Err(e) => {
            warn!("[eds] Failed to discover calendar sources: {e}");
            return vec![];
        }
    };

    if sources.is_empty() {
        debug!("[eds] No calendar sources found");
        return vec![];
    }

    let view_paths = Arc::new(StdMutex::new(HashSet::new()));
    let (time_range, query) = build_time_range_query_from_today();

    let mut handles = Vec::new();
    for source in &sources {
        let conn_clone = conn.clone();
        let state_clone = state.clone();
        let notifier_clone = notifier.clone();
        let source_uid = source.uid.clone();
        let query_clone = query.clone();
        let view_paths_clone = view_paths.clone();

        let handle = tokio::spawn(async move {
            match open_calendar(&conn_clone, &source_uid).await {
                Ok((calendar_path, bus_name)) => {
                    match create_view(&conn_clone, &bus_name, &calendar_path, &query_clone).await {
                        Ok(view_path) => {
                            {
                                let mut paths = match view_paths_clone.lock() {
                                    Ok(p) => p,
                                    Err(e) => {
                                        warn!("[eds] view_paths mutex poisoned, recovering: {e}");
                                        e.into_inner()
                                    }
                                };
                                paths.insert(view_path.clone());
                            }

                            if let Err(e) =
                                start_view(&conn_clone, &bus_name, &view_path).await
                            {
                                warn!("[eds] Failed to start view: {e}");
                                return;
                            }

                            if let Err(e) = spawn_view_monitor(
                                conn_clone,
                                bus_name,
                                view_path,
                                state_clone,
                                notifier_clone,
                                view_paths_clone,
                                time_range,
                            )
                            .await
                            {
                                warn!("[eds] View monitor error: {e}");
                            }
                        }
                        Err(e) => warn!("[eds] Failed to create view for {source_uid}: {e}"),
                    }
                }
                Err(e) => warn!("[eds] Failed to open calendar {source_uid}: {e}"),
            }
            debug!("[eds] View task for {source_uid} stopped");
        });

        handles.push(handle);
    }

    handles
}
```

**Step 3: Replace `monitor_eds_calendars` body with a call to `setup_calendar_views`**

Replace the entire body of `monitor_eds_calendars` (the loop and `Ok(())` at the end) with:

```rust
async fn monitor_eds_calendars(
    conn: Connection,
    state: Arc<StdMutex<EdsState>>,
    notifier: EntityNotifier,
) -> Result<()> {
    let handles = setup_calendar_views(&conn, state.clone(), notifier.clone()).await;

    {
        let mut st = match state.lock() {
            Ok(g) => g,
            Err(e) => {
                warn!("[eds] state mutex poisoned storing initial handles, recovering: {e}");
                e.into_inner()
            }
        };
        st.view_monitor_handles = handles;
    }

    Ok(())
}
```

**Step 4: Build and run EDS tests**

```bash
cargo build -p waft-eds-daemon 2>&1 | grep -E "^error"
cargo test -p waft-eds-daemon 2>&1 | tail -15
```

Expected: builds clean, all existing tests pass.

**Step 5: Commit**

```bash
git add plugins/eds/bin/waft-eds-daemon.rs
git commit -m "refactor(eds): extract setup_calendar_views for reuse in midnight rebuild"
```

---

## Task 5: Add midnight rebuild loop to the EDS plugin

**Files:**
- Modify: `plugins/eds/bin/waft-eds-daemon.rs`

**Step 1: Add the midnight loop to `monitor_eds_calendars`**

Replace the `Ok(())` at the end of `monitor_eds_calendars` (added in Task 4) with a `loop`:

```rust
async fn monitor_eds_calendars(
    conn: Connection,
    state: Arc<StdMutex<EdsState>>,
    notifier: EntityNotifier,
) -> Result<()> {
    let handles = setup_calendar_views(&conn, state.clone(), notifier.clone()).await;

    {
        let mut st = match state.lock() {
            Ok(g) => g,
            Err(e) => {
                warn!("[eds] state mutex poisoned storing initial handles, recovering: {e}");
                e.into_inner()
            }
        };
        st.view_monitor_handles = handles;
    }

    // Midnight loop: rebuild views once per day so the query window stays
    // anchored to today. Also purges events whose end_time is before the
    // new day to prevent stale entities from persisting in the daemon.
    loop {
        let secs = secs_until_eds_midnight();
        debug!("[eds] Next view rebuild in {secs}s (midnight)");
        tokio::time::sleep(std::time::Duration::from_secs(secs)).await;

        debug!("[eds] Midnight reached — rebuilding calendar views");

        // Compute new today midnight timestamp for stale-event pruning.
        let (new_time_range, _) = build_time_range_query_from_today();
        let new_today_midnight = new_time_range.start;

        // Abort old monitors and purge events that ended before the new day.
        {
            let mut st = match state.lock() {
                Ok(g) => g,
                Err(e) => {
                    warn!("[eds] state mutex poisoned during midnight rebuild, recovering: {e}");
                    e.into_inner()
                }
            };

            for handle in st.view_monitor_handles.drain(..) {
                handle.abort();
            }

            let stale_keys: Vec<String> = st
                .events
                .iter()
                .filter(|(_, event)| event.end_time < new_today_midnight)
                .map(|(key, _)| key.clone())
                .collect();

            for key in &stale_keys {
                st.events.remove(key);
            }

            if !stale_keys.is_empty() {
                debug!("[eds] Pruned {} stale events at midnight", stale_keys.len());
            }
        }

        // Notify daemon so it removes the pruned entities from its cache.
        notifier.notify();

        // Set up fresh views anchored to the new today.
        let new_handles = setup_calendar_views(&conn, state.clone(), notifier.clone()).await;

        match state.lock() {
            Ok(mut st) => st.view_monitor_handles = new_handles,
            Err(e) => {
                warn!("[eds] state mutex poisoned storing new handles, recovering: {e}");
                e.into_inner().view_monitor_handles = new_handles;
            }
        }

        debug!("[eds] Calendar views rebuilt for new day");
    }
}
```

**Step 2: Build**

```bash
cargo build -p waft-eds-daemon 2>&1 | grep -E "^error"
```

Expected: no errors. The compiler may warn about `unreachable` after the loop — that's fine since `monitor_eds_calendars` now loops forever (returning `!`). If it complains about the `Result<()>` return type, change it to `Result<!>` or add an unreachable annotation. More likely the `loop` without a `break` satisfies the `!` inference and compiles without changes.

> **Note:** If the compiler complains about return type, change the signature to `async fn monitor_eds_calendars(...) -> !` and remove the `Result<()>`. Update the call site in `main` accordingly: `if let Err(e) = ...` becomes just `tokio::spawn(async move { monitor_eds_calendars(...).await; })`.

**Step 3: Run all tests**

```bash
cargo test --workspace 2>&1 | tail -20
```

Expected: all tests pass.

**Step 4: Commit**

```bash
git add plugins/eds/bin/waft-eds-daemon.rs
git commit -m "feat(eds): rebuild calendar views at midnight to re-anchor query to new today"
```

---

## Task 6: Full build and final verification

**Step 1: Clean build**

```bash
cargo build --workspace 2>&1 | grep -E "^error"
```

Expected: no errors.

**Step 2: Run all tests**

```bash
cargo test --workspace 2>&1 | tail -20
```

Expected: all tests pass.

**Step 3: Quick smoke test (manual)**

Start the daemon and overview, then check that events for today are displayed correctly.
If you have `notify-send` available, confirm unrelated plugins still work:

```bash
notify-send "smoke test" "Notifications still working"
```

**Step 4: Commit if any fixups are needed, then done.**
