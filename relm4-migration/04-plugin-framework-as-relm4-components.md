# 04 — Redesign Plugin Framework: Plugins Provide Relm4 Components + Slot/Weight Metadata

## Goal

Replace the current “plugins return GTK widgets” framework with a **Relm4-first plugin framework** where each plugin provides:

- stable metadata (id, name, slot/column, weight/order),
- one or more **Relm4 components** to mount in the overlay UI,
- a typed message channel so the **central App router** can send events to plugin components.

Plugins remain **static after startup** (no unload/reload).

This step should introduce the new framework **without** migrating existing plugin UIs yet. Where necessary, use temporary adapters/stubs so the app remains buildable and tests pass.

## Changes (what you will do)

### A) Define plugin metadata + placement types (Relm4-friendly)

Create types that describe plugin placement in the overlay:

- `PluginId` (reuse the type introduced in step 03)
- `Slot` (Left / Right / Top) — reuse existing semantics, but define/host it in the new plugin framework module if needed
- `weight: i32` sorting rule: heavier goes lower (preserve current behavior)

**Measurable outcome:** You can express “place Bluetooth menu in Right column at weight 30” without constructing GTK widgets.

### B) Define the new plugin contract (trait)

Introduce a new Relm4-oriented plugin trait (name it however fits your codebase, e.g. `RelmPlugin`), roughly:

- `fn id(&self) -> PluginId`
- `fn name(&self) -> &'static str` (or `String` if dynamic)
- `fn placement(&self) -> PluginPlacement` (slot + weight)
- `fn init(&mut self, ctx: PluginInitContext) -> Result<(), PluginInitError>`
  - must be GTK-safe (no widget creation pre-GTK init)
  - may start DBus/background tasks that only send messages/events
- `fn mount(&mut self, mount: PluginMountContext) -> MountedPlugin`
  - responsible for instantiating the plugin’s Relm4 component(s)
  - runs only after GTK/adw is initialized
  - returns handles/controllers needed by the app router to talk to the plugin component

**Important constraint:** The plugin trait must remain GTK-friendly (no forced `Send + Sync`), consistent with the existing architecture direction.

### C) Define the “mounted plugin” handle shape (routing target)

Define how the App router sends messages to plugin components.

Two acceptable approaches (pick one and commit to it):

#### Option 1 (recommended): Typed per-plugin input senders collected behind an enum
- Each plugin’s mounted component yields an input sender/controller type.
- The plugin registry stores these as a `PluginOutputs` enum keyed by `PluginId`.
- Routing becomes a `match` on plugin id/message variant, then calls `sender.input(...)`.

Pros: compile-time safety; easy to test; refactor-friendly.  
Cons: you update the enum as plugins change (fine with 5 static plugins).

#### Option 2: Uniform trait object for “send message to plugin”
- Define `trait PluginEndpoint { fn send(&self, msg: PluginMsg); }`
- Store `Box<dyn PluginEndpoint>` per plugin.

Pros: less boilerplate.  
Cons: easier to get wrong; more runtime indirection; weaker typing.

Given the “fast automated tests” requirement and small plugin count, **Option 1 is preferred**.

### D) Implement a new plugin registry that mounts components

Introduce a new registry that:

1. Registers plugins statically at startup (as today).
2. Calls `init()` for each plugin before UI mounts (no GTK widget creation).
3. After Relm4/adw initialization, calls `mount()` to instantiate all plugin components once.
4. Exposes:
   - a sorted view of mounted plugin components by `Slot` then `weight`,
   - a routing table for the App router to deliver `PluginMsg` to the correct plugin component(s).

**Key behavior preserved:**
- Sorting by weight per slot.
- Plugins are static and mounted once.
- No GTK construction in `init()`.

### E) Transitional compatibility: keep old plugin system compiling

Because later steps will migrate plugin UIs one by one:

- Keep the existing plugin trait and widget-returning behavior compiling for now.
- Introduce the new Relm4 plugin framework alongside it.
- The app chooses *one* framework at runtime/build time:
  - either via feature flags (e.g. `relm4-app`),
  - or via a temporary “dual-path” main that chooses the new Relm4 app entrypoint.
- In this step, you may mount **stub components** for all plugins (e.g. labels per slot) to prove the registry + placement works, without porting actual UI.

**This step is successful even if plugins show placeholder components**, as long as the framework is in place and tested.

### F) Add fast tests for plugin placement + routing (non-UI)

Add unit tests that validate:

1. Sorting:
   - Given a set of plugin placements, the registry produces deterministic order by slot then weight.
2. Routing table:
   - Given a `PluginId` + `PluginMsg`, the registry returns the correct endpoint variant (or produces the expected error if missing).
3. “No GTK in init” guard (lightweight):
   - Enforce by convention + code review.
   - Optionally, add a test that `init()` can run without GTK init by using stub plugins that do no GTK work.

Avoid initializing GTK/adw in unit tests.

## Definition of Done (measurable)

- A new Relm4-oriented plugin framework exists with:
  - plugin metadata (id/slot/weight),
  - a plugin trait with `init()` and `mount()` split across the GTK init boundary,
  - a registry that mounts plugin components once and stores routing endpoints.
- The central App router (from step 03) can route a `PluginMsg` to a plugin endpoint **through the registry API** (even if plugin components are still stubs).
- Automated tests exist and pass for:
  - placement sorting logic,
  - routing/endpoint lookup logic.
- The application remains buildable and tests pass:
  - `cargo build`
  - `cargo test`
  - and for the Relm4 path (if feature-gated): `cargo build --features relm4-skeleton` or your chosen Relm4 feature.

## Verification

### Build
- `cargo build`
- `cargo build --features relm4-skeleton` (if still using the skeleton flag)  
  or the equivalent feature for the new Relm4 app entrypoint.

### Tests
- `cargo test`
- `cargo test --features relm4-skeleton` (if applicable)

### Manual smoke test
Run the Relm4 app entrypoint (skeleton/relm4 feature) and confirm:

- A window opens (adw styling).
- You see placeholder plugin components placed into the correct columns (Left/Right/Top).
- Ordering respects weight (heavier lower) within each column.

No DBus behaviors need to change in this step.

## Notes / Guardrails

- **No GTK widgets in `init()`**: plugins must not construct widgets prior to GTK initialization.
- Do not move GTK widgets across threads; continue using message passing for background work.
- Avoid dynamic plugin typing where possible—prefer typed enums for endpoints/messages.
- Keep legacy code compiling until later steps explicitly remove it.

## Follow-ups (next steps preview)

- Step 05: Implement the Relm4 overlay layout that mounts the registry’s plugin components into columns/slots.
- Step 06–08: Migrate the simplest plugins first (replace stub component with real component).
- Step 09+: Migrate Bluetooth menu (factory list if needed).
- Step 10+: Migrate Notifications plugin + DBus server + toast window semantics and add focused integration tests.