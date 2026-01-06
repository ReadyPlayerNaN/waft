# sacrebleui – Architecture Notes

This document captures the current architectural direction, especially around plugins, UI, and state flow. It’s meant for future “agents” (human or otherwise) working on this codebase.

---

## High‑level goals

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

- Plugins should be **self‑contained** providers of behavior and declarative UI.
- The UI layer should deal only with **generic concepts** (feature tiles, sliders, menus), not plugin internals.
- State flow should be **explicit and centralized**, minimizing hidden couplings and avoiding memory/leak patterns around callbacks or sinks.
- The design must remain **GTK‑friendly**: all GTK widgets live on the main thread; plugins must not be forced to be `Send + Sync` unless there is a compelling need.

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
  - Exposing a collection of `FeatureToggle`s describing its UI surface.

Current direction:

- **`Plugin` is *not* required to be `Send + Sync`.**
  - This is important to remain compatible with GTK and `Rc`‑based callbacks stored in `FeatureSpec`.
  - GTK widgets and `Rc` are not `Send`/`Sync`, so forcing plugins to be thread‑safe spills complexity everywhere.
- `initialize` / `cleanup` are async, but they are intended to run on the main (GTK) executor, not on arbitrary worker threads.

Long‑term, if a use‑case appears that truly needs multi‑threaded plugin management, we will revisit this and potentially split the trait into:
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

## UI Event Bus

To prevent cross‑layer entanglement (plugins knowing too much about models, models knowing about plugin lifetimes) we introduce a **UI event bus**:

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

- **Feature toggles** (`FeatureToggle` with `FeatureSpec` describing a toggle tile).
- **Other widgets** (sliders, buttons) represented as:
  - Additional `FeatureSpec` variants, or
  - Additional enums / specs parallel to `FeatureSpec` over time.
- **Menus**:
  - A `FeatureSpec` may be “contentful” (split tile) and open a details panel represented by a `MenuSpec` + child widgets.
  - User interactions in the menu are wired back to the plugin via callbacks stored inside the spec.

The plugin:

- Describes *what* to render and *which callbacks* to call via `FeatureSpec`.
- Handles actions in those callbacks (and may also emit `UiEvent`s if UI state must change).

The UI:

- Knows only how to:
  - Render `FeatureSpec` (including menus).
  - Call callbacks on user input.
  - Update its model when the event bus asks for changes.

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

1. **Declarative UI only in specs**
   - Keep `FeatureSpec` / `MenuSpec` purely declarative + callbacks.
   - Avoid embedding mutable, long‑lived plugin state inside them.

2. **Plugin state is plugin‑local**
   - Each plugin owns its own domain state.
   - To reflect state in the UI, send `UiEvent`s; do not directly manipulate models.

3. **Central, generic models**
   - `FeaturesModel` and similar are generic; they know nothing of plugin internals.
   - They update purely from `UiEvent`s and are reconstructed from `FeatureSpec`s plus current model state.

4. **Single, central event bus**
   - One event bus (conceptually) for UI changes.
   - Plugins get senders; UI gets one receiver.
   - This avoids reference cycles and makes plugin unloads safe.

5. **Main‑thread UI, no forced `Send + Sync` on plugins**
   - Do not assume plugins are thread‑safe.
   - Use async + background tasks for blocking work; keep plugin structs and GTK types on the main thread.

---

## Implementation notes / Next steps

- The codebase is being moved to:
  - `FeatureToggle` owning `FeatureSpec` (no lifetimes).
  - `Plugin` without `Send + Sync` bounds.
  - `DarkmanPlugin`:
    - Building `FeatureSpec`s that describe its toggle UI.
    - Handling actions in callbacks and background tasks.
    - Emitting UI events to a central event bus rather than directly editing UI models.

Future contributors should:

- Preserve the separation between plugins, specs, models, and event bus.
- Extend `UiEvent` instead of passing models directly into plugins.
- Be careful when introducing any new `Send + Sync` bounds that might conflict with GTK or `Rc`‑based structures.