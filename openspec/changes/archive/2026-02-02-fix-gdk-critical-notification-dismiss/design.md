## Context

The notifications plugin has two widget types that show dismissable notifications:
- `NotificationCard` - used in the notification center popover
- `ToastWidget` - used for toast popup notifications

Both use `gtk::Revealer` for show/hide animations and `GestureClick` controllers for user interaction. When a notification is dismissed:
1. The revealer animation starts (`set_reveal_child(false)`)
2. When animation completes, `connect_child_revealed_notify` fires and removes the widget from its parent
3. Gesture handlers may still be processing events on the now-destroyed widget

The codebase already uses `glib::idle_add_local_once` for deferred operations (see `trigger_window_resize()` in `main_window.rs`).

## Goals / Non-Goals

**Goals:**
- Eliminate GTK/Gdk CRITICAL assertions when dismissing notifications
- Ensure widget removal only happens when GTK is not processing events for that widget
- Prevent gesture handlers from accessing destroyed widgets
- Maintain existing animation behavior and user experience

**Non-Goals:**
- Changing the visual animation timing or style
- Modifying the notification store or D-Bus integration
- Adding new user-facing features

## Decisions

### Decision 1: Use `idle_add_local_once` for deferred widget removal

**Choice:** Defer widget removal using `glib::idle_add_local_once` instead of removing immediately in the revealer callback.

**Rationale:**
- `idle_add_local_once` schedules a callback to run after current event processing completes
- This ensures all gesture handlers have finished before widget destruction
- Pattern already used successfully in `trigger_window_resize()`

**Alternatives considered:**
- **Weak references in gesture handlers** - Would require checking validity on every event, adds complexity
- **Channel-based removal via store** - Over-engineered; the issue is timing, not architecture
- **Delay with timeout** - Arbitrary delays are fragile and waste time

### Decision 2: Use `hidden` flag to guard gesture handlers

**Choice:** Check the `hidden` flag at the start of gesture handlers before accessing widget properties.

**Rationale:**
- `toast_widget.rs` already has this pattern (`if *hidden_clone.borrow() { return; }`)
- `notification_card.rs` lacks this guard, causing the `widget.pick()` crash
- Simple, consistent fix across both widgets

**Alternatives considered:**
- **WeakRef for widget references** - More complex, `hidden` flag already exists and serves same purpose

### Decision 3: Single removal path (revealer callback only)

**Choice:** Only the revealer `connect_child_revealed_notify` callback should remove widgets. Parent containers should not also remove.

**Rationale:**
- Prevents double-removal race conditions
- Clear ownership: the widget controls its own lifecycle
- Parent updates can hide widgets (start animation) but not remove them

**Alternatives considered:**
- **Parent-controlled removal** - Creates race with animation callbacks, harder to reason about

## Risks / Trade-offs

**[Risk] Widget lingers briefly after animation** → Acceptable. The idle callback runs immediately after current event processing; delay is imperceptible (single frame at most).

**[Risk] Hidden flag becomes stale** → Mitigated by single removal path. Flag is only set true when initiating hide; removal always follows.

**[Trade-off] Slightly more complex callback** → Worth it for safety. The deferred removal pattern is already established in the codebase.
