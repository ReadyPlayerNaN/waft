# 12 — Remove Legacy GTK Paths and Flip Default to Relm4 Entry Point

## Goal

Make Relm4 + libadwaita (`adw`) the **only** supported UI path by:

- removing the legacy “pure GTK” overlay entrypoint and widget-based plugin UI framework,
- switching the default `main` entrypoint to the Relm4 application,
- ensuring DBus services + models are owned/started from the Relm4 app path,
- keeping the application **buildable** and **tests passing** at the end of the step.

This is the “cutover” step: after it, the project is fully Relm4-based.

## Scope

### Included
- Delete/retire legacy GTK overlay UI codepaths (old `main` path, old builders, old window plumbing).
- Delete/retire legacy plugin widget interfaces (plugins returning `gtk` widgets directly).
- Flip defaults:
  - `cargo build` builds the Relm4 app by default (no special feature flag required).
  - `cargo test` runs against the Relm4 app code layout by default (tests remain mostly UI-free).
- Ensure notifications DBus server ownership policy remains correct:
  - attempt replace `org.freedesktop.Notifications`
  - fail startup if cannot acquire.
- Ensure the toast window + overlay window behavior remains correct (manual smoke tests).

### Excluded
- New features.
- UI driver tests (expensive/brittle).
- CI integration (explicitly ignored for now).

## Changes (what you will do)

### A) Identify the “legacy path” boundaries and delete them

1. Remove the old overlay window construction code that is not used by the Relm4 app.
2. Remove any “dual main” selection logic (feature flag gating, runtime toggles) so that:
   - `main` always launches the Relm4 `adw` application.
3. Remove old modules that only exist to support the GTK widget composition path:
   - old plugin registry (widget-based),
   - overlay slot containers and builders that append raw `gtk::Box` widgets,
   - any adapter/shim layers created solely to keep both UIs alive earlier in the migration.

**Measurable:** after this step there is exactly one top-level UI entrypoint: the Relm4 one.

#### Guardrail
Do not delete:
- DBus ingress layer
- domain cores (notifications core, bluetooth model core)
- tests
unless you are replacing them with better equivalents in the same step.

### B) Remove legacy plugin trait(s) and old widget surfaces

Replace all remaining references to legacy plugin trait(s) with the Relm4 plugin framework contract established earlier:

- plugins provide Relm4 components (mounted once, static plugin set),
- placement via `Slot` + `weight`,
- message routing through the app router.

Actions:
1. Delete the legacy widget-returning methods and associated types (e.g. old `Widget`, old `Slot` if duplicated, etc.).
2. Update all plugin implementations to implement only the Relm4-oriented plugin trait(s).
3. Ensure any “external capabilities” are exposed through explicit typed handles/controllers, not by leaking widgets.

**Measurable:** there is no longer a code path where plugins “return raw GTK widgets” to be composed by the app.

### C) Flip default Cargo features / configuration

If earlier steps introduced feature flags like `relm4-app` or `relm4-skeleton`, you must:

- make Relm4 the default build without requiring flags,
- remove the old feature flag(s) or repurpose them for optional debugging only.

Concrete tasks:
1. Update `Cargo.toml` feature configuration so that:
   - Relm4/adw dependencies are normal dependencies (not feature-gated), unless you have a strong reason to keep them optional.
2. Ensure `cargo build` builds the Relm4 app.
3. Ensure `cargo test` passes without extra features.

**Measurable:** a new contributor can run `cargo run` and get the Relm4 app.

### D) Ensure DBus services start from the Relm4 app path

Confirm the DBus server startup (notably `org.freedesktop.Notifications`) is driven from the Relm4 app initialization path, not from legacy UI modules.

Tasks:
1. Ensure startup sequence is correct and respects GTK init boundaries:
   - DBus connections/tasks can start before widgets are created,
   - no GTK objects are created before GTK/adw init.
2. Ensure DBus tasks communicate via `AppMsg` sink/message routing only (no UI imports in DBus modules).
3. Ensure “fail startup if cannot own `org.freedesktop.Notifications`” behavior is preserved.

**Measurable:** removing the legacy path does not change DBus behavior and does not introduce GTK-init panics.

### E) Remove transitional debug hooks/adapters (if any remain)

By this stage, earlier steps may have added:
- debug injection hooks for Bluetooth events,
- debug buttons for “send overlay shown/hidden messages”,
- temporary logging of notification IDs for manual testing.

Decide for each:
- remove it, or
- keep it behind an explicit `debug-tools` feature.

**Measurable:** production/default build does not include debug-only UI clutter.

### F) Strengthen automated tests to protect the cutover

This step is a risky refactor because it deletes code. Increase protection using fast tests (non-UI) so regressions are caught.

Required test improvements:
1. **Compile-time coverage**
   - Ensure no tests or modules depend on the removed legacy plugin/widget types.
2. **Router smoke unit tests**
   - Ensure overlay gating still works (OverlayShown/Hidden → toast gating effects).
3. **Notifications core tests**
   - Must continue to pass (IDs, replacement semantics, close reasons, action ordering).
4. **DBus integration tests**
   - If you added DBus integration tests earlier, ensure they still pass unchanged after cutover.

Optional tests (recommended if cheap):
- A “registry contains exactly N plugins” test to ensure plugin registration didn’t silently drop a plugin during refactor (static plugins).

**Measurable:** `cargo test` provides confidence that removal didn’t break behavior.

## Definition of Done (measurable)

- `cargo build` succeeds with no extra feature flags.
- `cargo test` passes with no extra feature flags.
- There is a single application entrypoint and it is the Relm4 + `adw` one.
- Legacy GTK overlay entrypoint is removed.
- Legacy widget-based plugin framework is removed.
- All plugins are mounted as Relm4 components via the new plugin framework.
- DBus ownership policy remains correct:
  - attempts to replace `org.freedesktop.Notifications`
  - startup fails if it cannot acquire the name.
- Notifications overlay list + toast window remain functional in the Relm4 app.

## Verification

### Build
- `cargo build`

### Tests
- `cargo test`

### Manual smoke tests (curated, functional parity)

#### 1) Overlay / toast interaction gating
1. Start the app: `cargo run`
2. Ensure overlay is hidden.
3. Observe toast window:
   - visible/mapped,
   - if no notifications: blank and zero height.
4. Send notification while overlay hidden:
   - `notify-send "Toast test" "Should toast now"`
   - confirm toast appears.
5. Show overlay (your normal mechanism):
   - confirm toast window stops presenting toasts / is hidden per policy.
6. Send notification while overlay visible:
   - `notify-send "Overlay visible" "Should NOT toast now"`
   - confirm it appears in overlay list, not as toast.
7. Hide overlay and send another:
   - `notify-send "Overlay hidden" "Should toast again"`
   - confirm toast appears.

#### 2) DBus contract smoke test
1. Verify name ownership:
   - `busctl --user status org.freedesktop.Notifications`
2. Verify markup:
   - `notify-send "Markup test" "<b>bold</b> <i>italic</i>"`
   - confirm overlay renders markup.
3. Verify actions + signals:
   - start monitor:
     - `dbus-monitor --session "type='signal',interface='org.freedesktop.Notifications'"`
   - send action notification:
     - `notify-send --action=default=Open "Action test" "Click Open"`
   - click action in overlay or toast (depending on visibility)
   - confirm you observe:
     - `ActionInvoked`
     - then `NotificationClosed`

## Notes / Guardrails

- Keep the separation strict:
  - DBus ingress + domain cores are UI-free.
  - UI reacts to messages/events.
- Ensure GTK init rules remain satisfied:
  - no widget creation in plugin `init()` or pre-GTK init paths.
- Avoid “big bang refactor churn” inside this step:
  - the goal is deletion/cutover and wiring, not redesign.
- If removal reveals dead code in domain layers, remove it only if covered by tests.

## Follow-ups (post-cutover cleanups)

After this step, consider a short cleanup phase (not required for this migration plan step) to:
- run `cargo fmt` / `cargo clippy` (if you use them),
- update `AGENTS.md` or other architecture docs to reflect the Relm4 world,
- simplify module structure now that dual-path code is gone,
- review dependency features to reduce compile time.