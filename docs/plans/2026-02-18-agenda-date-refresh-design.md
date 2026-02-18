# Design: Agenda date refresh (sleep-to-deadline)

**Date:** 2026-02-18
**Branch:** larger-larger-picture

## Problem

The agenda widget calls `chrono::Local::now()` inside `update_events()`, but
`update_events()` is only triggered by entity changes, selection changes, or menu
state changes. If none of those happen overnight the "today" reference stays frozen —
so events from ereyesterday appear labelled "Today" and today's events appear labelled
"Tomorrow".

Secondary issue: the EDS plugin builds its CalendarView query once at startup
(`today_midnight → +30 days`). After midnight the window drifts, anchored to the
previous day rather than the new today.

---

## Solution overview

Two independent sleep-to-deadline timers, one per layer:

1. **Widget** — after every `update_events()`, arm a one-shot glib timer that fires at
   the next moment something visible would change.
2. **Plugin** — after initial view setup, run a tokio loop that sleeps until midnight,
   then rebuilds all CalendarViews with a fresh query.

No polling. Each layer wakes only when its display needs to change.

---

## Section 1 — Widget timer (`crates/overview/src/components/agenda.rs`)

### Next-boundary calculation

```
next_boundary = min(earliest future_event.end_time, next_midnight)
```

If there are no future events, `next_boundary = next_midnight`.

```rust
fn next_boundary_secs(future_events: &[(Urn, CalendarEvent)]) -> u64 {
    let now = chrono::Local::now().timestamp();
    let midnight = secs_until_next_midnight();
    future_events.iter()
        .map(|e| e.end_time)
        .filter(|&t| t > now)
        .min()
        .map(|t| ((t - now).max(1) as u64).min(midnight))
        .unwrap_or(midnight)
}

fn secs_until_next_midnight() -> u64 {
    let now = chrono::Local::now();
    let tomorrow_midnight = (now.date_naive() + chrono::Duration::days(1))
        .and_hms_opt(0, 0, 0)
        .expect("midnight is always valid")
        .and_local_timezone(chrono::Local)
        .earliest()
        .expect("tomorrow midnight is a valid local time");
    (tomorrow_midnight.timestamp() - now.timestamp()).max(1) as u64
}
```

### Timer lifecycle

- Store `Option<glib::SourceId>` in the agenda state struct.
- At the end of every `update_events()`:
  1. Cancel the previous timer (`source_id.remove()`).
  2. Compute `next_boundary_secs(future_events)`.
  3. Arm a new one-shot timer via `glib::timeout_add_seconds_local`.
  4. Store the new `SourceId`.

Use `Rc::downgrade` in the timer closure. If the widget has been destroyed when the
timer fires, `weak.upgrade()` returns `None` and the callback returns
`ControlFlow::Break` — no panic, no dangling reference.

---

## Section 2 — Plugin midnight rebuild (`plugins/eds/bin/waft-eds-daemon.rs`)

### Midnight loop

After initial view setup, spawn a tokio task:

```
loop:
  sleep until next local midnight
  abort all existing view-monitor JoinHandles
  purge stale events (end_time < new_today_midnight) from EdsState
    → notifier.notify() removal for each purged event
  rebuild all CalendarViews with fresh build_time_range_query_from_today()
  spawn new view-monitor tasks, store new JoinHandles
  calculate next midnight → repeat
```

### State changes

`EdsState` gains a `Vec<tokio::task::JoinHandle<()>>` to track view monitor handles.

The existing `build_time_range_query_from_today()` is reused unchanged.

View creation and monitor spawning are extracted into a reusable function called
both at startup and by the midnight task.

---

## Section 3 — Error handling & edge cases

### Widget

| Scenario | Behaviour |
|---|---|
| `next_boundary_secs` computes 0 | Clamped to 1 s — avoids tight spin |
| Widget destroyed while timer pending | `Rc::downgrade` guard returns `ControlFlow::Break` — silent no-op |

### Plugin

| Scenario | Behaviour |
|---|---|
| Midnight duration ≤ 0 (clock skew / DST) | Clamped to 1 s before sleeping |
| View rebuild fails (EDS D-Bus error) | Log error, retry on next midnight; stale events remain visible |
| View monitor task panicked before midnight | `JoinHandle::abort()` is a no-op — midnight loop continues unaffected |

---

## Testing

| Test | Where |
|---|---|
| `next_boundary_secs`: no events → returns midnight | unit, `agenda.rs` |
| `next_boundary_secs`: event ends before midnight → returns that interval | unit, `agenda.rs` |
| `next_boundary_secs`: event ends after midnight → returns midnight | unit, `agenda.rs` |
| `secs_until_next_midnight`: correct across DST spring-forward | unit, `agenda.rs` |
| `secs_until_next_midnight`: correct across DST fall-back | unit, `agenda.rs` |
| Existing EDS RRULE tests | unchanged — no logic change in expansion |

No new integration tests: the timer scheduling is too tightly coupled to the
glib/tokio runtimes to test in isolation.

---

## Files changed

| File | Change |
|---|---|
| `crates/overview/src/components/agenda.rs` | Add `refresh_timer`, `next_boundary_secs`, `secs_until_next_midnight`; schedule timer at end of `update_events()` |
| `plugins/eds/bin/waft-eds-daemon.rs` | Add `view_monitor_handles` to `EdsState`; extract view setup fn; spawn midnight rebuild loop |
