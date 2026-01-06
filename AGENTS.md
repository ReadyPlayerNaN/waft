# sacrebleui – Architecture Notes

This document captures the current architectural direction, especially around plugins, UI, and state flow. It’s meant for future “agents” (human or otherwise) working on this codebase.

---

## High‑level goals

- Keep the app **GTK‑friendly**: GTK widgets live on the main thread; plugin APIs must not force `Send + Sync`.
- Keep plugins **self‑contained** and **state-owning**: a plugin owns its long-lived state and UI/controller objects.
- Prefer **explicit state flow**: external producers should communicate via clear APIs (today: direct calls; later: a bus).
- Keep the UI layer mostly **composition + placement** (columns/slots), not business logic.

---

## Current plugin system (as implemented)

The codebase currently has a practical plugin system that predates the “FeatureToggle-first” architecture direction.

### `Plugin` trait

- Location: `src/plugins/plugin.rs`
- Not `Send` (`#[async_trait(?Send)]`), keeping it compatible with GTK and `Rc` closures.
- Plugins can expose:
  - `feature_toggles()` (declarative tiles; still evolving)
  - `widgets()` (concrete GTK widgets for the overlay UI)

### `Widget` and slots

- Location: `src/plugins/bindings.rs`
- Plugins return `Vec<Widget>`, where each `Widget` has:
  - `el: gtk::Box`
  - `weight: i32` (sorting; heavier goes lower)
  - `column: Slot` (`Left`, `Right`, `Top`)

The overlay UI collects widgets from the registry and appends them to the correct column based on `Slot`.

### Registration

- Registration is manual in `main.rs` via the plugin registry.
- The registry stores plugins behind `Arc<Mutex<Box<dyn Plugin>>>`, allowing the UI to query widgets synchronously during build.

---

## Notifications: app icon lookup (current limitation)

When rendering notifications, we may want to display an "app icon" when no explicit notification icon is provided.

Current strategy (intentionally minimal dependencies):

- Try to treat the notification app name / app id as a themed icon name using `gtk::IconTheme::has_icon`.
- Apply normalization (e.g. lowercase, whitespace → `-`, strip punctuation) and try again.
- Fall back to a default icon if no icon can be found.

Non-goals (for now):

- Do not add dependencies on `gio` / `GDesktopAppInfo` to resolve `.desktop` files and `Icon=` entries.
  - This means human-readable app names like "Slack" may not reliably map to installed app icons.
  - If we later need higher fidelity, we can introduce an optional desktop-file lookup layer (likely via `gio`) behind a small abstraction.

This keeps the notifications UI GTK-friendly and lightweight, while leaving room for a more robust resolver later.

---

## Core concepts

### `FeatureSpec`

- Acts like a **declarative “React component”** for a feature tile / control.
- Contains:
  - Static metadata: key, title, icon.
  - Initial UI state: `active`, `open`, `status_text`, optional `MenuSpec`.
  - Optional callbacks for user actions, e.g. `on_toggle`.
- It is intentionally **UI‑oriented** and **cloneable**:
  - It is safe to construct `FeatureSpec` values in plugins and pass them into the UI.
  - The UI is free to store, clone, and reinterpret them when reconstructing views.

**Key principle:** `FeatureSpec` should *not* own arbitrary plugin state; it’s a description of what and how to render, plus how to notify the plugin when something happens.

---

### `FeatureToggle`

- A lightweight wrapper that describes how a plugin exposes a single feature in the UI.
- **Owns** a `FeatureSpec` (by value), which is a snapshot of how the plugin wants that feature to appear.
- Contains:
  - `id: String` – stable identifier (often same as `FeatureSpec.key` or namespaced key).
  - `weight: i32` – sorting weight for ordering in the UI.
  - `el: FeatureSpec` – the declarative spec itself.

Because `FeatureToggle` owns its `FeatureSpec`:

- There are **no lifetimes** in this part of the API.
- Plugins simply build a fresh `FeatureSpec` whenever they are asked to expose their toggles.
- The UI layer can treat `Vec<FeatureToggle>` as **pure data**.

---

### `Plugin` trait

- Responsible for:
  - Naming itself.
  - Performing any initialization or cleanup work.
  - Exposing UI surfaces:
    - Today: **concrete widgets** via `widgets()` (placed into columns by `Slot`)
    - Also supported: **feature toggles** via `feature_toggles()` (declarative tiles; still evolving)

Current direction:

- **`Plugin` is *not* required to be `Send + Sync`.**
  - This is important to remain compatible with GTK and `Rc`‑based callbacks stored in UI specs.
  - GTK widgets and `Rc` are not `Send`/`Sync`, so forcing plugins to be thread‑safe spills complexity everywhere.
- `initialize` / `cleanup` are async, but they are intended to run on the main (GTK) executor, not on arbitrary worker threads.

Long‑term, if a use‑case appears that truly needs multi‑threaded plugin loading/unloading, we can revisit this and potentially split the trait into:
- A main‑thread UI part, and
- An internal worker part that *is* `Send + Sync` and communicates via channels.

---

## State model and data flow

A key design choice: **who owns the “true” state** and how changes propagate.

### 1. Plugins as the source of domain state

Each plugin owns its **domain‑specific state**:

- Example (`DarkmanPlugin`):
  - DBus interaction with `darkman`.
  - Internal notion of whether dark mode is currently enabled (either cached or re‑queried).
- The plugin decides when the domain state changes: DBus signal, user toggling via callback, etc.

The plugin does **not** directly mutate GTK widgets or internal fields of UI structs outside of its own `FeatureSpec` construction and callbacks.

### 2. UI as a reflection of plugin and global state

The UI layer owns **UI state**:

- `FeaturesModel` (and similar models) store:
  - Per‑feature `active` status.
  - Status text.
  - Whether a feature’s menu is expanded/collapsed.
- `build_features_section` uses:
  - A set of `FeatureSpec`s (from plugins).
  - The model’s current state.
- UI is **reconstructible** from model + `FeatureSpec`s.

When plugins change their domain state, they do not poke `FeatureSpec` instances in place. Instead, they notify the UI layer that “feature X is now active/inactive” or “text changed”, and the model is updated.

---

## UI Event Bus (planned / partial)

To prevent cross‑layer entanglement (plugins knowing too much about models, models knowing about plugin lifetimes) we introduce a **UI event bus**:

Note: the codebase currently has `UiEvent` defined, and some plugins accept an optional sender, but the primary, working integration mechanism today is still the plugin registry + widget composition.

### `UiEvent` (conceptual)

An enum that captures **what needs to change in the UI**, not *how* it is changed:

- `FeatureActiveChanged { key: String, active: bool }`
- `FeatureStatusTextChanged { key: String, text: String }`
- `FeatureMenuOpenChanged { key: String, open: bool }`
- (Future) slider updates, menu actions, etc.

This enum is **owned by the UI/application layer**, not by plugins.

### Event flow

1. **Initialization:**
   - The app creates a central `UiEvent` receiver + sender pair (e.g. an mpsc channel).
   - The app hands **cloned senders** to each plugin at construction or in `initialize`.

2. **Plugin side:**
   - When the plugin observes a domain change (e.g. DBus “mode changed to dark”):
     - It translates that into one or more `UiEvent`s (“feature `plugin::darkman` active = true”) and sends them.
   - When a user action callback fires (e.g. `on_toggle` in a `FeatureSpec`):
     - The callback can, inside the plugin, both:
       - Perform domain logic (e.g. call DBus to set dark mode).
       - Emit a `UiEvent` to request model/UI updates.

3. **UI side:**
   - A single central task listens to the `UiEvent` receiver.
   - It applies changes to `FeaturesModel` (or other UI models) accordingly:
     - `FeatureActiveChanged` → `FeaturesModel::set_active(key, active)`.
     - etc.

This means:

- `FeaturesModel` is **plugin‑agnostic**: it only understands generic events.
- Plugins never hold references to models, preventing lifetime / leak issues at plugin unload time.
- Unloading a plugin is straightforward:
  - Drop its sender(s).
  - Its tasks naturally end when they can no longer send events.
  - The model’s view of features can be recomputed the next time the features section is built (the plugin simply won’t contribute toggles).

---

## Plugin‑provided UI surfaces

Each plugin can provide one or more kinds of UI surfaces:

- **Concrete column widgets** via `Plugin::widgets()` returning `Vec<Widget>`:
  - The UI places these into `Slot::Left` / `Slot::Right` / `Slot::Top`.
  - Widgets are sorted by `weight` (heavier goes lower).
  - Plugins should generally return **the same widget instance** across calls if the widget contains state and must persist across UI rebuilds.
- **Feature toggles** via `Plugin::feature_toggles()` returning `Vec<FeatureToggle>`:
  - This is the longer-term declarative direction for quick settings tiles.
  - Not all existing UI is expressed this way yet.

The plugin:

- Either returns fully-built widgets (today’s stable path), or
- Describes declarative UI via specs (evolving path).
- Owns domain and view/controller state so it can persist and be externally controlled.

The UI:

- Composes widgets into slots/columns.
- Renders feature specs where appropriate.

---

## Threading model

- GTK widgets and most UI logic live on the **main thread**.
- Plugins currently:
  - Run their async lifecycle (`initialize` / `cleanup`) on the main executor.
  - Offload blocking IO using:
    - `spawn_blocking` for heavy/DBus operations.
    - Regular async tasks (`tokio::spawn`) that only use `Send` data.
- The `Plugin` trait is **not constrained** to be `Send + Sync`, to keep this GTK‑compatible.

If a future need arises for multi‑threaded plugin loading/unloading:

- We will introduce a dedicated worker abstraction and avoid forcing GTK objects or `Rc` data across threads.

---

## Design summary / Principles to follow

1. **Prefer stable plugin boundaries**
   - Plugins expose UI via `widgets()` (concrete widgets) and/or `feature_toggles()` (declarative tiles).
   - Keep plugin surfaces simple, clone-free where possible, and GTK-friendly.

2. **Plugin state is plugin‑local**
   - Each plugin owns its domain state and any UI/controller state it needs to persist across UI rebuilds.
   - The UI should not hold the “true” state; it composes what plugins provide.

3. **Concrete widgets can still have explicit state flow**
   - Even when returning widgets, avoid hidden couplings: expose explicit imperative APIs (handles) or events.
   - When a bus is used, prefer plugin-agnostic `UiEvent` variants.

4. **Event bus is optional, not mandatory**
   - Use `UiEvent` where it reduces coupling (especially for declarative feature tiles).
   - For purely widget-based plugins, an imperative API/handle is acceptable.

5. **Main‑thread UI, no forced `Send + Sync` on plugins**
   - Do not assume plugins are thread‑safe.
   - Use async + background tasks for blocking work; keep plugin structs and GTK types on the main thread.

---

## Implementation notes / Next steps

### Notifications pluginization (implemented)

- Notifications UI has been moved under `src/features/notifications/` and is now provided by a plugin.
- The notifications plugin:
  - **owns** the notifications controller/model/view so it persists across UI rebuilds,
  - returns the notifications UI as a left-column widget via `Plugin::widgets()`,
  - exposes an imperative **clear** capability intended to be callable “from the outside”.

Recommended structure (current pattern):

- `types.rs` – domain types (`Notification`, `NotificationIcon`, actions, snapshot types)
- `model.rs` – testable grouping/sorting model
- `view.rs` – GTK rendering
- `controller.rs` – wiring + imperative methods (`add/remove/clear`)
- `plugin.rs` – plugin glue + seeding data (until ingress is implemented)

### Feature toggles (still evolving)

- The codebase is still moving toward:
  - `FeatureToggle` owning `FeatureSpec` (no lifetimes),
  - `Plugin` without `Send + Sync` bounds.

Future contributors should:

- Preserve the separation between plugins, specs, models, and any future event bus.
- Prefer explicit, minimal coupling between plugins and UI composition.
- Be careful when introducing any new `Send + Sync` bounds that might conflict with GTK or `Rc`‑based structures.