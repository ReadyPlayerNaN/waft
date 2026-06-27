# Plan: Configurable clear-toasts-on-overview-open behavior

## Status

Implementation-ready plan, revised with oracle review.

## Goal

Add a config option controlling whether opening `waft-overview` clears currently visible toasts.

Desired behavior:

- default: **enabled**
- when overview opens:
  - currently visible toasts are removed immediately
  - canonical notifications remain in overview/store
- while overview remains open:
  - new notifications are dropped as toasts
  - they still remain in overview/store

## Confirmed product behavior

This is a **presentation policy** only.

It must not:
- dismiss canonical notifications
- change notification backend semantics
- change plugin/store/protocol behavior

## Recommended config location

Put this in the **overview app config**, not plugin config.

Reason:
- this behavior is purely about overview/toast presentation policy
- it belongs next to other overview-side presentation choices like toast position
- it should not leak presentation policy into the notifications backend/plugin

## Recommended config shape

Use the overview config/default TOML shape.

Recommended field:

```toml
clear_toasts_on_open = true
```

If the existing overview config groups toast-related fields under a section, keep it consistent with current layout. But avoid inventing a plugin-facing field for this.

## Repo-grounded config context

Current relevant files:

- `crates/overview/default.toml`
- `crates/overview/src/app.rs`
- `crates/overview/src/features/toasts/toast_manager.rs`
- `crates/overview/src/features/toasts/toast_window.rs`
- `crates/config` crate (for config types/loading if needed)

The overview already uses overview-side config for toast presentation concerns such as toast position, so this option should be added in the same config path.

## Design principles

1. Canonical notification state stays in the store/plugin layer.
2. Toasts are a transient presenter only.
3. “Clear on overview open” means:
   - clear presenter-local toast state
   - do **not** emit dismiss to backend
4. “Drop while overview open” remains active regardless of whether there were previous visible toasts.
5. Parent overview visibility remains the trigger.

## Non-goals

- no backend/plugin config changes
- no daemon/protocol/entity changes
- no replay queue for missed toasts after overview closes
- no changes to canonical notification retention semantics
- no toast policy changes outside this overview-open behavior

## Implementation plan

### Phase 1 — Add config field with default enabled

Add the overview-side config field.

Likely touched files:

- `crates/overview/default.toml`
- overview config struct/type definitions in `crates/config` or overview-local config accessors
- any parsing/defaulting tests for overview config

Requirements:

- default resolves to `true`
- missing field remains backward-compatible and behaves as enabled

### Phase 2 — Thread config into overview toast policy setup

In `crates/overview/src/app.rs`:

- load the new config field alongside existing toast position config
- pass the configured behavior into toast presentation setup

Avoid scattering config lookups deep inside widget code; prefer passing the policy once from app bootstrap.

### Phase 3 — Extend toast manager with explicit clear-on-open policy

In `crates/overview/src/features/toasts/toast_manager.rs`:

- add a stored boolean policy, e.g. `clear_on_overview_open: bool`
- keep existing `suppressed_by_overview` concept for dropping new toasts while overview is open

Add an explicit API for overview visibility transitions, for example:

- `set_overview_visible(visible: bool)`

Behavior when `visible == true`:

- if `clear_on_overview_open` is enabled:
  - immediately clear presenter-local toast state:
    - active toasts
    - pending queue
    - widget map / visible widgets
  - do **not** send dismiss/expire actions to backend
- always enable suppression so newly arriving toasts are dropped while overview is open

Behavior when `visible == false`:

- disable suppression
- do not replay dropped toasts

Important:
- clearing visible toasts must be treated as local presenter teardown, not notification dismissal

### Phase 4 — Ensure toast window fully disappears after clear-on-open

In `crates/overview/src/features/toasts/toast_window.rs` and/or `toast_manager.rs`:

- make sure clearing presenter-local state results in the toast window hiding completely
- no stale shell/window should remain
- hidden/removal path should be safe for currently displayed widgets and animation callbacks

If animation complicates teardown, prefer correctness over preserving a fancy exit animation for this policy path.

### Phase 5 — Preserve drop-while-open behavior for new notifications

In `toast_manager.rs`:

- keep or tighten the logic that drops new toast presentations while `suppressed_by_overview` is active
- ensure this applies both when clear-on-open is enabled and disabled

Behavior matrix:

#### `clear_toasts_on_open = true`
- opening overview clears current visible toasts
- new notifications while open are dropped as toasts
- overview/store still show notifications canonically

#### `clear_toasts_on_open = false`
- opening overview hides/suppresses toast window without clearing local toast state
- new notifications while open are still dropped as toasts
- on close, existing still-active transient toasts may reappear if still locally present

That makes the option specifically about **clearing existing toasts on open**, not about whether suppression while open exists.

### Phase 6 — Add focused regression coverage

Add tests at the correct seam.

#### Config tests
- missing field defaults to `true`
- explicit `false` is parsed correctly

#### Toast manager tests
- with `clear_on_overview_open = true`:
  - active/pending/widget state is cleared on overview open
  - no backend dismiss is emitted as part of local clear
- with `clear_on_overview_open = false`:
  - suppression flips on, but local toast state is retained
- in both modes:
  - new notifications while overview is open are dropped as toasts

#### Existing overview tests
- keep current notification-group regression coverage intact

If full GTK widget testing is heavy, extract small testable policy/state helpers for toast-manager behavior.

## File-by-file expected touch map

Primary expected files:

- `crates/overview/default.toml`
- config types/parsing/defaults in `crates/config` and/or overview config access path
- `crates/overview/src/app.rs`
- `crates/overview/src/features/toasts/toast_manager.rs`
- `crates/overview/src/features/toasts/toast_window.rs` (if needed)
- `crates/overview/src/components/mod.rs` only if test entry wiring needs updates

## Validation strategy

### Focused commands

```bash
cargo build -p waft-overview
cargo test -p waft-overview
cargo clippy -p waft-overview --all-targets -- -D warnings
```

### Reviewer validation

Run a reviewer pass after implementation focusing on:
- config placement correctness
- canonical state vs presenter-local state separation
- no accidental backend dismisses on clear-on-open
- correct defaulting behavior
- no regressions in overview/toast interaction

### Manual smoke checks

Under a real Wayland session:

1. with default config
   - show several toasts while overview is closed
   - open overview
   - verify all visible toasts disappear immediately
   - verify notifications still exist in overview
2. while overview remains open
   - trigger new notifications
   - verify no toast appears
   - verify notifications still appear in overview/store
3. close overview
   - verify dropped notifications are not replayed as toasts
4. set `clear_toasts_on_open = false`
   - show toasts
   - open overview
   - verify toast window hides but local clear-on-open policy is not applied
   - verify new notifications while open are still dropped as toasts

## Risks

### Risk 1 — accidental backend dismissal on local clear

Mitigation:
- keep local clear path separate from dismiss/expire action paths
- no action callback emission when clearing-on-open

### Risk 2 — stale widget removal/animation callbacks after local clear

Mitigation:
- ensure local teardown clears active/pending/widget state consistently
- prefer deterministic hide/cleanup over fancy animation in this policy path

### Risk 3 — config put in wrong layer

Mitigation:
- keep field in overview config only
- do not place it in notifications plugin config

### Risk 4 — conflating clear-on-open with suppression-while-open

Mitigation:
- keep them distinct in code and tests
- suppression while open remains baseline policy
- config only toggles whether existing visible toasts are cleared on open

## Acceptance criteria

This work is done when all are true:

1. new overview config field exists and defaults to `true`
2. opening overview with default config clears visible toasts locally without dismissing canonical notifications
3. notifications arriving while overview is open are dropped as toasts and not replayed later
4. setting the config to `false` disables only the clear-on-open part, not suppression-while-open
5. `cargo build/test/clippy -p waft-overview` pass
6. reviewer finds no blocking issues

## Oracle sign-off summary

Oracle judged the revised plan implementation-ready.

Key oracle conclusions incorporated:
- config belongs to overview-side presentation config
- keep plugin/store changes out of scope
- separate “clear current toasts on open” from “drop new toasts while open”
- keep canonical notification state and transient presenter policy strictly separate
