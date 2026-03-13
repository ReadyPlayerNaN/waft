# Launcher Toggle Visibility Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Running `waft-launcher` when already visible toggles it hidden (with animation reversal); running it when hidden does a fresh open.

**Architecture:** GTK single-instance (`adw::Application`) delivers every subsequent `waft-launcher` invocation to `connect_activate` on the existing process. Add a predicate to `LauncherWindow` exposing the private `animating_hide` flag, then update `connect_activate` to choose show/hide/reverse based on current animation state.

**Tech Stack:** Rust, GTK4, libadwaita (`adw::TimedAnimation`)

---

### Task 1: Expose `is_animating_hide` on `LauncherWindow`

**Files:**
- Modify: `crates/launcher/src/window.rs`

No unit test — GTK widget code requires a running display. Verify by inspection.

**Step 1: Add the predicate after the `hide()` method**

In `crates/launcher/src/window.rs`, after the closing `}` of `hide()` (line 226), add:

```rust
/// Returns `true` while a hide animation is in progress.
pub fn is_animating_hide(&self) -> bool {
    self.animating_hide.get()
}
```

**Step 2: Build to confirm no errors**

```bash
cargo build -p waft-launcher
```

Expected: compiles cleanly.

**Step 3: Commit**

```bash
git add crates/launcher/src/window.rs
git commit -m "feat(launcher): expose is_animating_hide predicate on LauncherWindow"
```

---

### Task 2: Toggle behaviour in `connect_activate`

**Files:**
- Modify: `crates/launcher/src/app.rs`

**Step 1: Replace the body of the `connect_activate` closure**

In `crates/launcher/src/app.rs`, the `connect_activate` closure currently starts at line 127 and its body is lines 129–135:

```rust
app.connect_activate(move |_| {
    // Reset query and search entry text
    *query_for_activate.borrow_mut() = String::new();
    win_for_activate.reset();
    // Populate results immediately if entities are already in store;
    // this also clears the loading spinner when data is present.
    update_results(&win_for_activate, &index_for_activate.borrow(), &cmd_index_for_activate.borrow(), "", &usage_for_activate.borrow(), rank_by_usage, max_results);
    win_for_activate.show();
    win_for_activate.grab_focus();
});
```

Replace with:

```rust
app.connect_activate(move |_| {
    if win_for_activate.window.is_visible() {
        if win_for_activate.is_animating_hide() {
            // Mid hide-animation: reverse back to visible from current opacity.
            win_for_activate.show();
        } else {
            // Fully visible or mid show-animation: start/continue fade-out.
            win_for_activate.hide();
        }
        return;
    }
    // Fully hidden: reset search and open fresh.
    *query_for_activate.borrow_mut() = String::new();
    win_for_activate.reset();
    update_results(&win_for_activate, &index_for_activate.borrow(), &cmd_index_for_activate.borrow(), "", &usage_for_activate.borrow(), rank_by_usage, max_results);
    win_for_activate.show();
    win_for_activate.grab_focus();
});
```

**Step 2: Build**

```bash
cargo build -p waft-launcher
```

Expected: compiles cleanly.

**Step 3: Manual smoke test**

```bash
WAFT_DAEMON_DIR=./target/debug cargo run --bin waft-launcher &
# Launcher opens → run again:
cargo run --bin waft-launcher
# Expected: launcher hides with fade-out animation.
cargo run --bin waft-launcher
# Expected: launcher shows fresh with fade-in, empty search.
```

Also verify mid-animation reversal by invoking the binary during the 100 ms window — the animation should smoothly reverse direction.

**Step 4: Commit**

```bash
git add crates/launcher/src/app.rs
git commit -m "feat(launcher): toggle visibility on re-invoke with animation reversal"
```
