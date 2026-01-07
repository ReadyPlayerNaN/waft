# Relm4 Migration Plan — Overview & Goals

This folder breaks the “rewrite the entire project to use Relm4 + libadwaita (`adw`)” effort into small, specific, measurable, achievable, realistic tasks.

Each step is a standalone milestone with a clear “Definition of Done” and ways to verify it:
- the app remains **buildable**
- **tests pass**
- core behaviors remain correct (with additional manual smoke tests where automation is impractical)

## Scope (what is included)

This migration covers the **entire project**:

- Overlay UI (currently built with direct `gtk` composition)
- Plugin framework (plugins become Relm4 components)
- DBus services + domain models (especially `org.freedesktop.Notifications`)
- Notifications UI behavior including the “toast window” behavior described below
- Tests (heavy focus on fast automated tests; UI tests are generally avoided)

## Non-goals (to keep the effort bounded)

- Pixel-perfect visual parity in every corner from day one (functionally equivalent is the priority).
- Full UI test automation (snapshot/UI-driver tests tend to be expensive and brittle).
- Runtime plugin unload/reload (plugins remain **static after startup**).

## Key behavioral requirements (must not regress)

### 1) Notifications DBus ownership policy

- On startup, the app must attempt to **replace** an existing owner of `org.freedesktop.Notifications`.
- If it cannot acquire the name, startup **fails** (the app exits) rather than running without the name.

### 2) Notifications “toast window” behavior (important)

The notifications plugin is responsible for toast rendering:

- A “toast window” (separate surface) exists that pops up notifications **while the main overlay is hidden**.
- The toast window is currently **always visible** until the main overlay is displayed.
- When there are **zero notifications**, the toast window is:
  - blank, and
  - **zero height** (effectively invisible, but still mapped/visible as a window).

**Target parity for migration:**
- Preserve these semantics unless a later migration step explicitly changes them.

### 3) Plugin model

- Plugins are **static after startup**.
- Plugins must become **Relm4 components** (no returning raw GTK widgets as the primary plugin UI surface).

### 4) Threading / GTK boundaries

Even with Relm4:
- GTK widgets stay on the main thread.
- Background work (DBus listening, model updates, timers) must not mutate GTK widgets directly.
- Prefer message passing into Relm4 components and/or a central router component.

## Architectural target (what we’re aiming for)

### Central App Message Router

The target architecture is a Relm4 application with a central router:

- One top-level Relm4 component acts as the “App” and central message router.
- DBus services and background tasks send events/messages into the App component.
- The App component routes messages to the appropriate plugin component(s) or updates shared domain state.

This keeps boundaries explicit and avoids plugins needing to know about other plugins’ internals.

### Plugin framework as component registry

Instead of “plugins return GTK widget trees”, plugins will:
- register themselves with metadata (id, weight, slot/column placement),
- provide a **Relm4 component** to mount in the UI,
- optionally expose non-UI “capabilities” through well-defined handles (e.g. controller methods), but without cross-thread GTK access.

## Testing strategy (heavy automation focus)

### What we optimize for
- **Fast unit tests** for domain models (notification grouping/sorting, Bluetooth model logic, etc.).
- **Integration tests** for DBus service behavior where feasible (message transformations, DBus protocol correctness, state changes).
- Minimal, curated **manual smoke tests** for UI-specific behavior that is costly to automate (toast window, overlay show/hide interactions).

### What we generally avoid
- Full end-to-end UI tests that drive GTK events and assert pixels/layout: they are often slow and brittle.

## Step format in this plan

Each step file (`relm4-migration/NN-step-name.md`) should include:

- **Goal**: what the step accomplishes.
- **Changes**: what code and structure is expected to change.
- **Definition of Done**: objective criteria.
- **Verification**:
  - how to build,
  - how to run tests,
  - any specific behaviors to manually confirm (if applicable).

## Global “Definition of Done” for each step

A step is complete only when:

1. `cargo test` passes (and any additional test commands defined in the step).
2. The app builds in dev profile (`cargo build`).
3. Any step-specific smoke checks pass.

If a step introduces temporary adapters or transitional modules, it must:
- be clearly documented in the step file,
- have a follow-up step scheduled to remove the adapter.

## Notes on sequencing

The plan is designed to:
- establish a Relm4 + `adw` app skeleton early,
- migrate core models and DBus-facing logic with strong test coverage,
- then migrate UI surfaces incrementally (overlay + toast window),
- while keeping the project in a runnable, testable state at each milestone.