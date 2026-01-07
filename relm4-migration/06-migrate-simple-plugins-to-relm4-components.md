# 06 — Migrate Simple Plugins to Real Relm4 Components

## Goal

Replace placeholder/stub plugin components (from steps 04–05) with **real Relm4 + libadwaita (`adw`) components** for the *simplest* plugins first, while keeping the app buildable and tests fast.

This step is intentionally scoped to **the simplest plugin UIs** (not Bluetooth menu, not Notifications/toast window). The purpose is to prove the end-to-end plugin-component contract:
- plugin `init()` does non-UI setup only,
- plugin `mount()` constructs widgets/components (after GTK init),
- plugin message typing stays **inside the plugin** via a `PluginSpec` + `Input` enum (Option 1.5A),
- the app wiring layer can acquire typed handles (`registry.get::<Spec>()`) and send typed inputs (`handle.send(&Input::...)`) to plugin components,
- plugins can emit messages/events back to the app/router (if needed).

## Scope

### Included
- 1–3 simplest plugins (choose those with minimal UI and minimal DBus complexity).
- Their UI rendered in the overlay via Relm4 component(s).
- Minimal typed message routing to these plugins.

### Excluded
- Bluetooth plugin menu migration (later step).
- Notifications plugin migration (later step).
- Any changes to DBus ownership policy or DBus interfaces.

## Changes (what you will do)

### A) Choose “simple plugins” and lock scope

Pick which plugins qualify as “simple” and list them in this step’s PR/commit description and/or in a short note at the top of the file you change (recommended). Criteria:
- UI is primarily a single tile/row/button/switch.
- No complex list rendering.
- No complex background ingestion loops required to show correct UI.

**Measurable selection outcome:**
- At least **one** plugin migrated fully to a real Relm4 component.
- At most **three** plugins migrated in this step.

### B) Implement real Relm4 components for the selected plugins

For each selected plugin:

1. Create a dedicated module for its Relm4 UI component, e.g.
   - `src/relm4_app/plugins/<plugin_name>/mod.rs`
   - `src/relm4_app/plugins/<plugin_name>/component.rs`
   - `src/relm4_app/plugins/<plugin_name>/model.rs` (optional if pure)

2. Implement the component with:
   - a minimal model that represents current UI state,
   - a message enum for user actions and incoming routed messages,
   - a view using `adw` widgets where appropriate (e.g. `adw::ActionRow`, `adw::PreferencesGroup`, `adw::SwitchRow`, etc.).

3. Update the plugin implementation to:
   - keep `init()` GTK-free (no widget construction),
   - instantiate and mount the component(s) in `mount()`.

4. Ensure the plugin placement (slot/weight) remains correct.

**UI guidance:**
- Prefer `adw` widgets for rows/settings-like UI.
- Keep component lifecycle stable (no rebuild loops).
- If you need to reflect domain state, update the model via messages rather than mutating widgets from background tasks.

### C) Wire typed message routing for these plugins (Option 1.5A)

Ensure the app wiring layer can send messages to these migrated plugins in a typed way **without** introducing a centralized enum-of-plugins/messages.

Requirements for each migrated plugin:
1. Define a plugin-owned input enum:
   - `enum Input { ... }` (derive `Debug`, `Clone`, `PartialEq`, `Eq` where reasonable).
2. Define a plugin-owned `PluginSpec` implementation:
   - `struct Spec; impl PluginSpec for Spec { type Input = Input; fn id() -> PluginId { ... } ... }`
   - Keep the `Input` and `Spec` in the plugin module (avoid “app knows plugin internals”).
3. Ensure `mount()` returns an endpoint that accepts the plugin’s `Input`.

Minimum for this step:
- one “ping” / “refresh” / “set state” message path from app → plugin component, delivered via:
  - `if let Some(handle) = registry.get::<plugin::Spec>() { handle.send(&plugin::Input::Ping)?; }`
- one user-action path from plugin component → app router (e.g. “user toggled X”).

This validates the direction without requiring DBus integrations yet.

### D) Add fast automated tests (heavy focus)

Add/extend tests at two levels:

#### 1) Pure unit tests (required)
For each migrated plugin, add at least one unit test that validates the plugin’s “update” logic / reducer without GTK:

Examples (adapt to plugin behavior):
- toggling a boolean flips model state and produces the correct outgoing app message/effect,
- receiving an incoming `Input::SetActive(bool)` (or equivalent) updates model state correctly,
- placement metadata remains stable (id/slot/weight),
- `PluginSpec` identity is stable (`Spec::id()` matches the plugin’s registered id).

These tests should not initialize GTK and should not require a main loop.

#### 2) Integration-ish tests without UI (recommended)
If the plugin interacts with a domain model (even if not DBus), add a test that exercises the model boundary:
- verify that an input event maps to an app-level message (`AppMsg`) and/or a plugin-level effect.
- verify that the plugin emits a router message on user action.

Avoid any test that creates windows or renders widgets.

### E) Keep legacy / unmigrated plugins stable

For plugins not migrated in this step:
- keep their placeholder/stub components mounted (from step 05),
- do not change their behavior,
- do not refactor their DBus integration yet.

This ensures each step is small and safe.

## Definition of Done (measurable)

- At least **one** simple plugin is fully migrated from a stub to a real Relm4+adw component.
- The migrated plugin’s overlay UI is visible and usable (at minimum: shows correct title/controls and responds to user interaction).
- The migrated plugin:
  - does not create GTK widgets in `init()`,
  - mounts widgets/components only in `mount()`.
- There is at least **one fast unit test per migrated plugin** that validates its update logic without GTK initialization.
- The app remains buildable and tests pass:
  - `cargo build`
  - `cargo test`
  - and for the Relm4 app path: `cargo build --features relm4-app` and `cargo test --features relm4-app` (or your chosen feature flag).

## Verification

### Build
- `cargo build`
- `cargo build --features relm4-app` (or your chosen Relm4 feature flag)

### Tests
- `cargo test`
- `cargo test --features relm4-app` (or the equivalent)

### Manual smoke tests (small, targeted)
Run:
- `cargo run --features relm4-app`

Verify:
1. The overlay window opens and shows the migrated plugin UI in the correct slot.
2. Interact with the migrated plugin UI control(s) (toggle/click):
   - UI updates immediately (model → view),
   - the action routes through the component update path (not direct widget mutation from background threads).
3. If you implemented an app → plugin message (e.g. refresh):
   - trigger it via a temporary debug action (menu item / keybind / button) and confirm the plugin updates.
   - (If you add temporary debug UI, track it for removal in a later cleanup step.)

## Notes / Guardrails

- Do not introduce DBus or background tasks that mutate GTK widgets directly.
- Do not block in component update handlers; use async tasks that send messages back into the component/app.
- Keep changes per plugin incremental:
  - first reproduce the visible UI structure,
  - then wire the minimal message routing,
  - then add tests.
- Avoid adding “render-time side effects” (e.g. starting tasks from view construction). Start tasks from explicit init/mount points.

## Follow-ups (next steps preview)

- Step 07: Standardize the DBus/background-task → `AppMsg` ingress path and add integration tests around it.
- Step 08–09: Migrate the Bluetooth plugin UI (menu + dynamic device list, likely using Relm4 factory components).
- Step 10+: Migrate Notifications plugin, including:
  - DBus server interface correctness,
  - overlay visibility gating (toasts pop when overlay hidden),
  - toast window semantics (always visible; zero-height when empty),
  - strong unit + integration tests for notification semantics.