# 05 — Implement Relm4 Overlay Window Layout (Columns/Slots) and Mount Plugin Components

## Goal

Create the real Relm4 + libadwaita overlay UI surface that:

- builds an **overlay window** using `adw`/GTK4 via Relm4,
- lays out **Top / Left / Right** slots (columns/areas) equivalent to today’s overlay composition,
- mounts the plugin components produced by the new plugin registry (step 04),
- preserves ordering semantics (slot + `weight`),
- wires **overlay shown/hidden** into the central app router messages so toast gating can be implemented later.

This step establishes the main “host” UI for all plugin components. It can still show placeholder/stub components for plugins not migrated yet.

## Changes (what you will do)

### A) Add a real Relm4 App entrypoint (migration path)

If you still have a `relm4-skeleton` feature from step 02, evolve it into a real Relm4 app entrypoint, e.g.:

- feature flag rename from `relm4-skeleton` → `relm4-app` (optional but recommended),
- or keep `relm4-skeleton` but make it the real overlay host.

The key requirement: **default build remains working**, and the Relm4 path becomes increasingly functional.

### B) Create the overlay “App component” and window

Implement a Relm4 component (the central App router component) that owns:

- the plugin registry instance (registered plugins),
- mounted plugin components and their controllers/inputs (from step 04),
- overlay visibility state (at least: shown/hidden),
- the actual overlay window widget tree.

#### Required widget tree (conceptual)

Create an overlay layout with three placement areas:

- `Top`: a horizontal container at the top
- `Left` column: vertical container
- `Right` column: vertical container

A simple structure can be:

- Root: `adw::ApplicationWindow` (or a suitable overlay window type you already use)
- Main vertical box:
  - Top slot container (horizontal)
  - Content row:
    - Left slot container (vertical)
    - Spacer / center area (optional, depending on current UI)
    - Right slot container (vertical)

The exact center content is project-specific; it can be a spacer for now if the UI is purely side columns.

### C) Mount plugin components by slot + weight

Use the new plugin registry’s sorted placements to mount plugin components into the correct slot container.

Rules to preserve:

- `Slot` determines which container receives the component.
- Within each slot:
  - sort by `weight` ascending (lighter at top) OR descending (heavier lower) — match existing semantics:
    - **Heavier goes lower**.
- Mount each plugin component exactly once (plugins are static).
- The overlay should not reconstruct plugin widgets on every message; use Relm4 component instances mounted once.

If some plugins are still stubs, the stub component should visibly identify itself (plugin id + slot + weight) so you can verify placement quickly.

### D) Overlay visibility plumbing (for later toast gating)

Implement overlay show/hide state transitions that emit messages into the central router.

Minimum requirement in this step:

- When the overlay is shown, dispatch `AppMsg::OverlayShown`.
- When the overlay is hidden, dispatch `AppMsg::OverlayHidden`.

How you detect this depends on your windowing approach:
- if you have explicit “show overlay” actions, send messages there;
- if you rely on window signals, wire them to Relm4 messages.

Do **not** implement toast window behavior yet in this step; just ensure the plumbing exists and can be unit-tested (see tests section).

### E) Add fast automated tests (non-UI)

Add unit tests that do not require GTK initialization:

1. **Placement-to-layout mapping test** (pure):
   - Given a list of plugin placements, assert the computed slot buckets contain the right plugin ids in the correct order.
   - This should reuse the registry sorting logic from step 04, but it’s acceptable to test the overlay layout helper separately if needed.

2. **Overlay visibility reducer test** (pure):
   - If you implemented routing via the reducer from step 03, add/extend tests:
     - sending `OverlayShown` yields the expected effect(s) (e.g. `SetToastGating { enabled: false }`).
     - sending `OverlayHidden` yields `SetToastGating { enabled: true }`.

Avoid any tests that instantiate `gtk::Application`, `adw::ApplicationWindow`, or run a main loop.

### F) Minimal manual smoke tests (UI)

Because this step introduces a real window and layout, include a short manual check.

## Definition of Done (measurable)

- The Relm4 app entrypoint builds and runs, showing an overlay window.
- The overlay window contains three placement areas: Top, Left, Right (even if some are empty).
- Plugin components (stubs are fine) are mounted into the correct slot area.
- Ordering within each slot matches: **heavier weight goes lower**.
- Overlay show/hide triggers the correct `AppMsg` messages (at minimum via logging or temporary counters) without panics.
- `cargo build` succeeds and `cargo test` passes (including new unit tests).
- The default (non-Relm4) build path still builds and tests pass (until you later flip defaults).

## Verification

### Build

- `cargo build`
- `cargo build --features relm4-app` (or your chosen Relm4 feature flag)

### Tests

- `cargo test`
- `cargo test --features relm4-app` (or the equivalent)

### Manual smoke test

Run the Relm4 app:

- `cargo run --features relm4-app`

Confirm:

1. The overlay window opens successfully with adw styling.
2. You can visually identify the **Top / Left / Right** areas (temporary borders/labels are acceptable for this step).
3. Each plugin stub/component appears in the expected column.
4. Reordering sanity check:
   - if you have two stubs in the same slot with different weights, the heavier is below the lighter.
5. Toggle overlay visibility (whatever your current mechanism is):
   - showing the overlay results in `OverlayShown` being dispatched,
   - hiding results in `OverlayHidden`.
   - (Temporary debug logging is acceptable; remove it in a later cleanup step.)

## Notes / Guardrails

- Keep GTK construction strictly on the GTK thread (Relm4 will naturally enforce this if you follow its patterns).
- Do not introduce any periodic polling to keep the UI updated.
- Do not yet implement the toast window in Relm4; only the overlay window and message plumbing.
- Avoid rebuilding plugin components on state changes; plugin components should remain mounted once and update via messages/model changes.
- Keep the Relm4 UI code isolated under a module namespace (e.g. `src/relm4_app/`) so future steps can migrate plugins incrementally.

## Follow-ups (next steps preview)

- Step 06: Migrate the simplest plugin(s) from stub to real Relm4 components.
- Step 07: Add a typed message bridge from DBus/background tasks into `AppMsg`.
- Step 08+: Migrate Bluetooth menu (possibly with factory components for device lists).
- Step 10+: Migrate Notifications plugin including:
  - DBus server integration,
  - toast window semantics (always visible, zero-height when empty),
  - overlay visibility gating (toasts pop only when overlay hidden),
  - strong unit/integration tests for notification behavior.