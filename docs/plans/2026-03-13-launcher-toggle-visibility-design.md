# Launcher Toggle Visibility Design

**Date:** 2026-03-13

## Problem

Running `waft-launcher` when it is already running and visible should toggle it to hidden. The launcher uses GTK single-instance (`adw::Application`), so every subsequent invocation fires `connect_activate` on the existing process. Currently `connect_activate` always shows the window.

## Design

### Animation reversal

The existing `show()` and `hide()` methods both use `animation_progress` as `value_from`, so they support starting from mid-animation. The toggle must handle four states:

| Window state | Action |
|---|---|
| Fully visible, not animating | Start fade-out |
| Mid show-animation | Reverse: start fade-out from current opacity |
| Mid hide-animation | Reverse: start fade-in from current opacity |
| Fully hidden | Reset + fresh open |

### Changes

**`crates/launcher/src/window.rs`**

Add a public predicate to expose the private `animating_hide` flag:

```rust
pub fn is_animating_hide(&self) -> bool {
    self.animating_hide.get()
}
```

**`crates/launcher/src/app.rs`**

Replace the unconditional show block inside `connect_activate` with a toggle check:

```rust
app.connect_activate(move |_| {
    if win_for_activate.window.is_visible() {
        if win_for_activate.is_animating_hide() {
            // Reverse: cancel fade-out and fade back in from current opacity
            win_for_activate.show();
        } else {
            // Reverse: cancel fade-in (if mid-show) or start fade-out
            win_for_activate.hide();
        }
        return;
    }
    // Fully hidden → reset and fresh open
    *query_for_activate.borrow_mut() = String::new();
    win_for_activate.reset();
    update_results(...);
    win_for_activate.show();
    win_for_activate.grab_focus();
});
```

No changes to `show()` or `hide()` themselves — their existing guards and animation logic are sufficient:
- `hide()` guards against `!is_visible() || animating_hide` (prevents double-hide)
- `show()` clears `animating_hide` before playing (cancels an in-progress hide cleanly)

## Out of scope

- No reset of search state when reversing an in-progress hide
- No change to focus-loss or Escape hide behaviour
