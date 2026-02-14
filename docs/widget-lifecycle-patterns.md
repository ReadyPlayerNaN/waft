# Widget Lifecycle Patterns

This document describes the GTK widget lifecycle patterns used in Waft to prevent crashes, flickering, and stale state when widgets are created, animated, and removed.

## Problem: GTK Widgets and Destroyed State

GTK4 widgets can be in various states during their lifecycle. Two scenarios cause crashes or critical assertions:

1. **Gesture handlers fire on destroyed widgets.** When a user clicks or swipes a widget that is mid-animation-out, the handler calls methods like `widget.pick()` or `widget.parent()` on a widget whose tree position is already invalid.

2. **Widget removal during event processing.** If a widget is removed from the DOM while GTK is still processing events on it (e.g., inside a gesture callback), GTK triggers critical assertions or segfaults.

Both situations arise commonly in notification cards, which can be dismissed via click, right-click, close button, or action button -- all while a slide-out animation is running.

## Pattern 1: Hidden Flag Guard

**Where used:** `crates/overview/src/components/notification_card.rs`

A `Rc<RefCell<bool>>` flag named `hidden` is stored alongside the widget. Every gesture handler checks this flag before doing anything.

### How it works

1. The widget starts with `hidden = false`.
2. When any dismissal action fires (close button, right-click, left-click default action), the handler:
   - Checks `if *hidden_ref.borrow() { return; }` to bail out if already hidden.
   - Sets `*hidden_ref.borrow_mut() = true` before starting the hide animation.
   - Starts the revealer animation (`set_reveal_child(false)`).
   - Emits the output event (Close, ActionClick).
3. Any subsequent gesture events on the same widget see `hidden == true` and return early.

### Code example (from notification_card.rs)

Close button handler (lines 191-200):
```rust
let hidden_ref = hidden.clone();
close_btn.connect_clicked(move |_| {
    if *hidden_ref.borrow() {
        return;
    }
    *hidden_ref.borrow_mut() = true;
    revealer_ref.set_reveal_child(false);
    if let Some(ref cb) = *on_output_ref.borrow() {
        cb(NotificationCardOutput::Close(urn_clone.clone()));
    }
});
```

Left-click handler (lines 232-260) -- additionally guards `widget.pick()` and `widget.parent()` traversal:
```rust
left_click.connect_pressed(move |gesture, _n_press, x, y| {
    if *hidden_ref.borrow() {
        return;
    }
    // Safe to call pick() and traverse parent chain because hidden == false
    if let Some(widget) = gesture.widget() {
        if let Some(picked) = widget.pick(x, y, gtk::PickFlags::DEFAULT) {
            // ... traverse to check if a Button was clicked ...
        }
    }
    *hidden_ref.borrow_mut() = true;
    // ... emit events, start hide animation ...
});
```

Action button handlers (lines 155-158) also check the flag:
```rust
action_btn.connect_clicked(move |_| {
    if *hidden_ref.borrow() {
        return;
    }
    // ... emit action event ...
});
```

### When to use this pattern

Use the hidden flag whenever a widget has:
- Multiple gesture handlers that can trigger dismissal (click, right-click, close button).
- An animation between "user requests dismissal" and "widget is removed from DOM."
- Methods like `pick()` or `parent()` that fail on destroyed/orphaned widgets.

## Pattern 2: Revealer Animation with Deferred Resize

**Where used:** Notification cards, notification groups, events calendar, feature grid menus.

Layer-shell windows do not automatically resize when content changes. Revealers animate content in/out, but the layer-shell window retains its old size. A deferred resize must be triggered after the animation completes.

### How it works

1. Content is placed inside a `gtk::Revealer`.
2. When content should appear: `revealer.set_reveal_child(true)`.
3. When content should hide: `revealer.set_reveal_child(false)`.
4. A `connect_child_revealed_notify` callback fires when the animation completes.
5. Inside this callback, `trigger_window_resize()` is called.
6. `trigger_window_resize()` itself uses `idle_add_local_once` to defer the actual resize until after GTK finishes current event processing.

### The resize chain

```
revealer animation completes
  -> connect_child_revealed_notify fires
    -> trigger_window_resize() called
      -> idle_add_local_once schedules deferred resize
        -> window.set_default_size(WIDTH, -1) executes
          -> layer-shell recalculates window height from content
```

### Code (from main_window.rs, lines 36-46)

```rust
pub fn trigger_window_resize() {
    WINDOW_RESIZE_CALLBACK.with(|cb| {
        if let Some(ref callback) = *cb.borrow() {
            let callback = callback.clone();
            gtk::glib::idle_add_local_once(move || {
                callback();
            });
        }
    });
}
```

The actual resize callback (main_window.rs, lines 264-269):
```rust
set_window_resize_callback(move || {
    window_clone.set_default_size(OVERLAY_WIDTH_PX, -1);
});
```

### Usage sites

| Component | File | Purpose |
|-----------|------|---------|
| NotificationCard | `notification_card.rs:181` | Resize after card reveal/hide animation |
| NotificationGroup | `notification_group.rs:174` | Resize after expand/collapse older notifications |
| EventsComponent | `events.rs:107` | Resize after calendar grid reveal/hide |
| FeatureGrid | `feature_grid.rs:130` | Resize after menu revealer animation |
| MainWindow | `main_window.rs:147` | Resize after overlay show/hide animation |

### When to use this pattern

Use deferred resize via revealer whenever:
- Content is added/removed inside a layer-shell window.
- An animation changes the effective height of the content area.
- Direct `set_default_size` would fire during the animation, causing layout jumps.

## Pattern 3: Idle-Deferred Rebuild

**Where used:** `crates/overview/src/components/calendar_grid/calendar_component.rs`

When multiple entity store updates arrive in rapid succession, rebuilding the UI for each one causes flicker and wasted work. A coalescing pattern defers the rebuild.

### How it works

1. A `rebuild_scheduled: Rc<Cell<bool>>` flag starts as `false`.
2. When an entity update arrives:
   - If `rebuild_scheduled` is already `true`, return immediately (a rebuild is pending).
   - Set `rebuild_scheduled = true`.
   - Schedule the actual rebuild via `glib::idle_add_local_once`.
3. The deferred callback:
   - Sets `rebuild_scheduled = false`.
   - Performs the full UI rebuild.

### Code (from calendar_component.rs, lines 131-152)

```rust
let rebuild_scheduled = Rc::new(Cell::new(false));

Rc::new(move || {
    if rebuild_scheduled.get() {
        return;
    }
    rebuild_scheduled.set(true);
    // ... clone references ...
    glib::idle_add_local_once(move || {
        rebuild_scheduled_idle.set(false);
        Self::rebuild_grid(/* ... */);
    });
})
```

### When to use this pattern

Use idle-deferred rebuild when:
- Multiple rapid-fire updates can arrive for the same UI region (e.g., entity store broadcasts).
- Each rebuild is expensive (removes and recreates child widgets).
- Only the final state matters, not intermediate states.

## Pattern 4: Popover Close Deferral

**Where used:** `crates/overview/src/ui/main_window.rs`

When a GTK popover closes, the window briefly loses focus before the main surface regains it. Without deferral, the `is_active_notify` handler would hide the entire overlay during this transient focus loss.

### How it works

1. A `popover_recently_closed: Rc<Cell<bool>>` flag tracks transient state.
2. When the menu store detects a popover close (had popover, now doesn't):
   - Set `popover_recently_closed = true`.
   - Schedule a deferred check via `idle_add_local_once`.
3. The `is_active_notify` handler checks this flag:
   - If `popover_recently_closed == true`, skip hiding (the deferred handler will decide).
4. The deferred handler:
   - Clears the flag.
   - Checks if the window regained focus. If so, do nothing. If not, hide the overlay.

### Code (from main_window.rs, lines 193-221)

```rust
if had_popover && !has_popover {
    popover_recently_closed_for_sub.set(true);
    // ... clone references ...
    gtk::glib::idle_add_local_once(move || {
        recently_closed_flag.set(false);
        if window_ref.is_active() || animating_hide.get() {
            return;
        }
        // Window lost focus to external app, hide overlay
        animating_hide.set(true);
        animation.set_value_from(progress.get());
        animation.set_value_to(0.0);
        animation.play();
    });
}
```

### When to use this pattern

Use popover close deferral when:
- A popover or modal within a layer-shell window causes transient focus loss on close.
- The window has a "hide on focus loss" behavior that would fire incorrectly.

## Summary

| Pattern | Mechanism | Solves |
|---------|-----------|--------|
| Hidden flag guard | `Rc<RefCell<bool>>` checked in every gesture handler | Gesture handlers on destroyed/animating-out widgets |
| Revealer + deferred resize | `connect_child_revealed_notify` + `idle_add_local_once` | Layer-shell window not resizing after content change |
| Idle-deferred rebuild | `Rc<Cell<bool>>` + `idle_add_local_once` | Flicker from rapid entity store updates |
| Popover close deferral | `Rc<Cell<bool>>` + `idle_add_local_once` | False overlay hide during popover close focus transition |

All patterns share the principle of deferring work via `glib::idle_add_local_once` to let GTK finish processing the current event cycle before taking action that changes the widget tree.
