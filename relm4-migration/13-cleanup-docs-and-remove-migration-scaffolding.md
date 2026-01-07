# 13 — Cleanup: Docs, Refactors, and Remove Migration Scaffolding

## Goal

Finish the migration by removing temporary scaffolding, consolidating module boundaries, and updating documentation so the project is clearly and cleanly a **Relm4 + libadwaita (`adw`)** application.

This is a “quality & maintainability” step:
- no new user-facing features,
- no behavior changes unless explicitly listed and tested,
- reduce complexity introduced during incremental migration (adapters, feature flags, debug hooks, duplicated types).

At the end of this step:
- the codebase has one clear UI architecture,
- docs match reality,
- the project is easier to contribute to,
- the app builds and **all tests pass**.

## Scope

### Included
- Remove migration-era scaffolding:
  - compatibility shims/adapters
  - stub components
  - dual-routing glue that is no longer needed
  - temporary debug actions/UI used for event injection or id logging
- Refactor and consolidate modules:
  - ensure DBus ingress is UI-free
  - ensure domain cores are UI-free and well-tested
  - ensure Relm4 components are clearly separated from domain logic
- Documentation updates:
  - update architecture notes to reflect Relm4
  - document the plugin framework as “plugins provide components”
  - document toast window semantics and overlay gating in the new system
- Light refactors to reduce churn and improve readability (no redesign).

### Excluded
- UI-driver tests / pixel assertions / screenshot tests
- CI work (explicitly ignored for now)
- Runtime plugin unload/reload (still not required)
- Feature work unrelated to the migration

## Changes (what you will do)

### A) Remove migration scaffolding and dead code

1. Delete unused migration-only modules and types, such as:
   - placeholder/stub plugin components (if any remain)
   - “compat layers” that convert old widget-based plugin APIs to new component APIs
   - feature flags or conditional compilation blocks that are no longer used
   - transitional enums/variants in `AppMsg`/`PluginMsg` that were only for intermediate steps

2. Remove debug-only UI and hooks introduced for migration, for example:
   - “inject fake DBus event” buttons
   - “force overlay shown/hidden” debug actions (if not part of final UX)
   - temporary logging of notification IDs used only for manual testing
   - any “test-only” public APIs that leaked into production

**If you still want debug tooling**, keep it behind an explicit `debug-tools` feature flag and ensure:
- it is off by default,
- it does not change release behavior,
- it is documented in `docs/` (how to enable and what it does).

**Definition of Done for this subsection**
- `rg`/search finds no references to removed legacy scaffolding.
- There is no dead, unreachable code path for “old UI”.

### B) Consolidate module boundaries (clean layering)

Enforce a clear layering that makes it hard to regress into GTK-thread violations:

1. **DBus layer (UI-free)**
   - `zbus` interface implementations and name acquisition policy
   - domain ingress calls
   - mapping of domain events to app messages (or a narrow sink interface)
   - MUST NOT import `gtk`, `adw`, `relm4`, `glib`

2. **Domain core layer (UI-free)**
   - notifications core (IDs, replacement semantics, close reasons, action ordering, snapshots)
   - bluetooth snapshot/model core (device/adapter state, update semantics)
   - MUST NOT import `gtk`, `adw`, `relm4`, `glib`, `zbus` types

3. **Relm4 UI layer**
   - top-level app router component
   - overlay window component and slot layout
   - plugin components (notifications overlay list, toast window component, bluetooth menu, other plugins)
   - interacts with domain cores only via messages and snapshots/events

Concrete refactor tasks:
- Move any stray UI logic out of domain modules.
- Move any stray domain logic out of UI components into core modules.
- Ensure “message translation” boundaries are narrow and unit-testable.

**Definition of Done for this subsection**
- Domain core crates/modules compile without UI imports.
- DBus modules compile without UI imports.
- UI modules compile without DBus imports except through explicit, narrow types/events (or via `AppMsg`).

### C) Normalize message routing conventions

Over the migration, message routing can become inconsistent (multiple sinks, multiple conversion points). Consolidate:

1. Ensure there is exactly one central path for external events into UI:
   - DBus ingress → `AppMsg` (or router input message)
   - background tasks → `AppMsg`
   - plugin components → app router outputs (`AppMsg` requests)

2. Ensure there is exactly one path for app→plugin communication:
   - `AppMsg` routed through the app/router to typed plugin endpoints

3. Remove duplicate event types if the project has both:
   - `UiEvent` and `AppMsg` doing the same job
   - old “feature toggle events” and new “plugin msg” events

If you keep `UiEvent` as a compatibility layer:
- document it as deprecated in code comments
- provide one conversion point
- schedule removal (or remove it in this step if no longer needed)

**Definition of Done for this subsection**
- No two parallel event buses exist “by accident”.
- Conversion points are singletons (one module/function responsible).

### D) Documentation updates (make docs match the new world)

Update or create documentation so a new contributor understands the Relm4 architecture quickly.

Required doc updates:
1. `AGENTS.md` (or equivalent architecture notes)
   - Replace “plugins return GTK widgets” with “plugins provide Relm4 components”.
   - Keep and emphasize the GTK init boundary rule:
     - no widget creation prior to GTK init (`init()` vs `mount()` split).
   - Keep and emphasize “no GTK from background tasks”.
   - Update any guidance that references old UI composition.
   - Document toast semantics and overlay gating in the Relm4 system.

2. `README.md` (if present)
   - update run/build instructions to the new default entrypoint
   - add a section for the DBus notification server smoke test

3. Migration docs
   - mark migration steps as completed (optional)
   - keep `relm4-migration/` as historical record OR delete it:
     - If you keep it, add a short header saying it’s historical.
     - If you delete it, ensure key architectural decisions are preserved in permanent docs.

4. Developer notes
   - Document how to add a new plugin component:
     - where to put it
     - how to register it
     - how to route messages to it
   - Document testing strategy:
     - what belongs in unit tests (domain core)
     - what belongs in DBus integration tests (protocol/signal correctness)
     - what belongs in manual smoke tests (window behavior, zero-height semantics)

**Definition of Done for this subsection**
- Someone can add a new plugin following docs without reading the entire codebase first.
- Docs no longer mention the removed legacy GTK widget plugin API.

### E) Test suite polish (fast-first, regression-resistant)

You want heavy focus on fast automated tests. This final step should ensure tests are:
- fast enough to run constantly during development,
- structured to protect the key behavioral contracts.

Tasks:
1. Ensure domain core tests cover the parity contract:
   - notification id generation
   - `replaces_id` semantics
   - close reasons (by call / by user)
   - action invoked ordering: `ActionInvoked` then closed
   - capability strings list

2. Ensure DBus integration tests (if present) still run reliably:
   - on an isolated bus if possible
   - no reliance on the user’s system notification daemon
   - minimal timing flakiness (avoid sleeps; prefer awaiting signals with timeouts)

3. Ensure router tests exist and pass:
   - overlay shown/hidden gating toggles toast gating correctly
   - routing to plugin endpoints is deterministic

4. Remove tests that implicitly depended on legacy modules.

**Definition of Done for this subsection**
- `cargo test` is fast and stable (no frequent flakes).
- Key behaviors are protected by unit/integration tests rather than manual checks alone.

### F) Optional: Reduce dependency surface and compile times

Only do this if it’s straightforward and doesn’t destabilize the project:

- Remove unused GTK/Relm4/adw features.
- Remove unused crates that were only needed for legacy UI.
- If you added “both stacks” at any point, ensure only one remains.

**Definition of Done for this subsection**
- `Cargo.lock`/dependencies reflect the new architecture; no obvious dead crates remain.

## Definition of Done (overall, measurable)

- The project has a single Relm4 + `adw` UI architecture; no legacy overlay/widget-plugin path remains.
- All remaining migration scaffolding is removed (or isolated behind `debug-tools` and off by default).
- Module boundaries are clean:
  - DBus layer is UI-free
  - domain core layer is UI-free
  - UI layer is responsible only for rendering + message handling
- Documentation reflects current reality (no stale “widget-returning plugins” guidance).
- The app is buildable and tests pass:
  - `cargo build`
  - `cargo test`

## Verification

### Build
- `cargo build`

### Tests
- `cargo test`

### Manual smoke tests (curated, minimal)

These should be the same “high-value” checks from earlier steps, confirming there were no regressions during cleanup:

#### 1) DBus ownership + notification semantics
1. Start the app.
2. Verify ownership:
   - `busctl --user status org.freedesktop.Notifications`
3. Send notifications:
   - `notify-send "Smoke" "Hello"`
   - `notify-send "Markup test" "<b>bold</b> <i>italic</i>"`
4. Actions:
   - `dbus-monitor --session "type='signal',interface='org.freedesktop.Notifications'"`
   - `notify-send --action=default=Open "Action test" "Click Open"`
   - click action in overlay/toast (depending on visibility)
   - confirm `ActionInvoked` then `NotificationClosed`

#### 2) Overlay ↔ toast gating + zero-height semantics
1. With overlay hidden, ensure toast window is mapped/visible.
2. With zero notifications, toast window is blank and **zero height**.
3. Send notification while overlay hidden:
   - toast appears.
4. Show overlay:
   - toast presentation is gated off.
5. Send notification while overlay shown:
   - appears in overlay list, not as toast.
6. Hide overlay again:
   - toast presentation resumes for new notifications.

## Notes / Guardrails

- Cleanup refactors should be small and safe; avoid redesigning the architecture in this step.
- Do not weaken typing in routing (avoid dynamic `Any`-style message passing); it reduces testability.
- Keep GTK init/thread boundaries explicit and documented.
- If you remove `relm4-migration/` docs, ensure permanent docs retain key decisions:
  - plugin component contract
  - DBus ownership policy
  - toast window semantics
  - testing strategy
"""