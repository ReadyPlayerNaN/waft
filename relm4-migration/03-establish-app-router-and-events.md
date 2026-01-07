# 03 — Establish Central App Router and Event Types (Mostly Non-UI)
+
+## Goal
+
+Introduce the **central Relm4 “App router”** concept at the type/module level, focusing on **message/event types** and **pure reducers** (non-UI logic). This step lays the foundation for routing DBus and plugin events through a single place without yet migrating the existing UI.
+
+This step should be safe and fast:
+- no plugin UI migration yet
+- no window/overlay behavior changes yet (beyond compiling new modules)
+- heavy focus on unit-testable logic and stable interfaces
+
+## Changes (what you will do)
+
+### A) Create core message/event types
+
+Create a new module namespace for Relm4 app-wide routing, e.g.:
+- `src/relm4_app/`
+  - `events.rs`
+  - `router.rs`
+  - `mod.rs`
+
+Define, at minimum:
+
+1. `AppMsg`
+   - The top-level Relm4 message enum for the application.
+   - Must include variants for:
+     - overlay visibility changes (shown/hidden)
+     - plugin-directed messages (namespaced by plugin id)
+     - notifications DBus ingress (Notify/Close/ActionInvoked-related, at least as internal events)
+     - “toast window” visibility gating events (derived from overlay shown/hidden)
+
+2. `PluginId`
+   - A stable identifier type for plugins (string newtype or enum).
+   - Must support consistent formatting and comparisons.
+
+3. `PluginMsg`
+   - A plugin-scoped message type used by the router when sending messages to plugins.
+   - Two viable patterns:
+     - `PluginMsg` is an enum-of-enums (one variant per plugin), or
+     - `PluginMsg` is opaque and routed with `(PluginId, Box<dyn Any>)` (NOT recommended: harder to test).
+
+Prefer the enum-of-enums approach for testability and refactoring safety.
+
+4. `UiEvent` compatibility (transitional)
+   - If the project already has `UiEvent`, decide one of:
+     - **Option 1:** Keep `UiEvent` and add a conversion `impl From<UiEvent> for AppMsg`
+     - **Option 2:** Freeze `UiEvent` and start routing via `AppMsg` directly (leave `UiEvent` for old code only)
+
+This step should not delete `UiEvent`. It should introduce a **clear mapping strategy**.
+
+### B) Add pure “router reducer” logic (no GTK)
+
+Implement a pure reducer that embodies routing rules without constructing widgets:
+
+- Input: current app routing state + an incoming `AppMsg`
+- Output:
+  - updated routing state
+  - a list of “effects” describing what should happen next (still non-UI)
+
+Example effect types (choose what fits your codebase):
+- `RouterEffect::SendToPlugin { plugin: PluginId, msg: PluginMsg }`
+- `RouterEffect::SetToastGating { enabled: bool }`
+- `RouterEffect::InvalidateToastLayout` (if you decide toast height should be recomputed)
+
+Key rule: **no GTK types** in the reducer or effect types.
+
+#### Required behavior encoded in reducer
+
+1. Overlay gating drives toast behavior:
+   - When overlay becomes **shown** → toast gating becomes **disabled** (toast should not pop).
+   - When overlay becomes **hidden** → toast gating becomes **enabled**.
+
+2. The routing layer must be able to carry “ingress” events from DBus to the notifications plugin:
+   - (Exact shape may be refined later) but the type plumbing must exist now.
+
+### C) Add unit tests for reducer behavior (fast)
+
+Add unit tests that execute in a headless environment:
+
+1. Overlay gating test:
+   - Send `AppMsg::OverlayShown` → assert effects include `SetToastGating { enabled: false }`
+   - Send `AppMsg::OverlayHidden` → assert effects include `SetToastGating { enabled: true }`
+
+2. Basic plugin routing test:
+   - Send an app-level message intended for a plugin (e.g. `AppMsg::ToPlugin { plugin, msg }`)
+   - Assert a `SendToPlugin` effect is produced with the correct plugin id and message.
+
+These tests should not initialize GTK and should not require a running main loop.
+
+## Definition of Done (measurable)
+
+- New module(s) exist defining `AppMsg`, `PluginId`, and plugin-directed message routing.
+- A pure reducer exists that:
+  - updates routing state deterministically
+  - produces non-UI effects
+  - encodes overlay → toast gating rules
+- Unit tests exist for:
+  - overlay gating rule(s)
+  - plugin routing rule(s)
+- The app remains buildable and all tests pass with:
+  - default features
+  - `relm4-skeleton` feature (from step 02), if present
+
+## Verification
+
+### Build
+- `cargo build`
+- `cargo build --features relm4-skeleton` (if step 02 introduced this)
+
+### Tests
+- `cargo test`
+- `cargo test --features relm4-skeleton` (if applicable)
+
+### Manual smoke test (optional in this step)
+No UI behavior should change yet, but you may optionally run:
+- `cargo run --features relm4-skeleton`
+
+Confirm it still launches (no regressions from new modules/types).
+
+## Notes / Guardrails
+
+- Keep the router/reducer layer **GTK-free**.
+- Do not introduce cross-thread assumptions; this layer is “logic only”.
+- Do not migrate any plugin UI yet.
+- Prefer explicit, typed messages over dynamic dispatch for plugin messages.
+- Treat this step as the “public API boundary” for future migration work:
+  - future DBus code should translate into `AppMsg`
+  - future plugin components should receive messages routed from the app router
+
+## Follow-ups (next steps preview)
+
+- Step 04: Introduce the “plugin component contract” (metadata + component factory) without changing UI layout.
+- Step 05+: Start migrating one plugin at a time into Relm4 components, beginning with the simplest plugin(s), then Bluetooth, then Notifications + toast window.