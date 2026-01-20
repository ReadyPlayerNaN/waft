# sacrebleui – Architecture Notes

## Agent rule: clarification before implementation

Before implementing any user-requested change, always ask for clarification first. Do not start coding until the user confirms the key behavioral decisions (especially around DBus ownership, threading/main-loop boundaries, and public API/data model changes).

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



---

## Threading model + GTK initialization (must-follow)

### GTK init boundary (this has caused crashes before)

In this app, plugins are initialized **before** GTK is initialized.

That means:

- `Plugin::initialize()` **MUST NOT** create any GTK objects (`gtk::Box::builder()...build()`, `gtk::Label`, `gtk::Button`, etc).
- Creating GTK widgets in `initialize()` can panic with:
  - `GTK has not been initialized. Call gtk::init first.`

**Allowed in `initialize()`**
- DBus connections / subscriptions
- async tasks (as long as they don’t touch GTK)
- initializing pure Rust state (models, caches)
- creating channels

**Not allowed in `initialize()`**
- any GTK widget construction
- any code path that *implicitly* constructs GTK widgets (e.g. building a `MenuSpec` from a widget you construct there)

**Where to construct GTK widgets instead**
- Lazily, in `Plugin::widgets()` and/or `Plugin::feature_toggles()`, which run during UI build after GTK is initialized.
- If you need long-lived widget instances, construct them on-demand the first time `widgets()`/`feature_toggles()` is called and store them in plugin-owned main-thread state.

### Tokio / Send boundary

- GTK widgets are **not** `Send` / `Sync`.
- Anything moved into `relm4::tokio::spawn(...)` must be `Send`.
- Therefore:
  - Do **not** store GTK widgets inside state guarded by `relm4::tokio::sync::Mutex` if that state is moved into Tokio tasks.
  - Split plugin state into:
    - **Send-safe state** (domain/cache, DBus data) for background tasks, and
    - **GTK main-thread state** (widgets, UI handles) accessed only from the GTK thread.

### Updating UI from async tasks

- Never mutate GTK widgets from a Tokio task.
- Instead, send `UiEvent`s to the UI/event bus (preferred), or schedule a main-thread callback (glib main context) that mutates widgets.

### UI rendering strategy for submenu-like plugin UIs (React-ish; MUST FOLLOW)

When a plugin owns a long-lived, contentful submenu/details panel (e.g. a `MenuSpec` backed by a plugin-owned GTK widget), follow this rule:

- Treat the external service (DBus/BlueZ/etc.) as the **source of truth**.
- Keep a plugin-owned **model snapshot** of the external state (e.g. per-device connected status).
- Keep a stable mapping of domain IDs → GTK widget handles:
  - e.g. `HashMap<DeviceId, DeviceRowWidgets>` where `DeviceRowWidgets` contains the `gtk::Switch`, status label, etc.
- On state changes, update **only the affected widgets** (set switch state, sensitive flag, status text), rather than rebuilding the entire submenu.

Why:
- Rebuilding large widget trees on every signal causes flicker and wasted work.
- Triggering repaints from synchronous spec-generation paths (`feature_toggles()`/`widgets()`) can create feedback loops (render → events → render), leading to high CPU and an unresponsive overlay.
- Incremental updates keep the overlay responsive and prevents “a plugin can hang the entire app”.

Operational guidance:
- Prefer stable ordering (do not reorder rows on connect/disconnect) to avoid churn; only add/remove rows when the device set changes.
- If you need a transient UI state (connecting/disconnecting), represent it explicitly in the model and clear it only when the external source reports the final state.
- Apply UI updates on the GTK thread only (via the UI event pump or a scheduled GTK callback), never from background tasks.

### Wake-on-demand repaint queue for DBus-driven menus (no polling)

For DBus-driven UIs, you often need “immediate” updates when signals arrive, without:

- touching GTK from background tasks (not allowed), and
- adding a periodic poll on the GTK thread (undesirable).

Use a **wake-on-demand invalidate queue**:

1) **Background/DBus task (Send-only):**
- Decode DBus signals and update the plugin’s Send-safe model/cache.
- Enqueue an “invalidate” token (e.g. adapter id / device id) into a Send-safe queue.
  - Use `Arc<std::sync::Mutex<VecDeque<InvalidateKey>>>` or similar.
- Do **not** call any GTK APIs here.

2) **GTK thread drain + coalesce (scheduled once per burst):**
- Keep an `AtomicBool scheduled` that is set to `true` only by the GTK thread when it actually schedules a drain.
- When the GTK thread observes new invalidations and `scheduled` is `false`, schedule exactly one GTK callback (e.g. `invoke_local`) to:
  - drain the queue,
  - de-duplicate keys (e.g. `HashSet`),
  - apply incremental widget updates only for affected rows/menus,
  - set `scheduled` back to `false`.

Rules:
- Never schedule repaints from `feature_toggles()`/`widgets()` loops.
- Never capture GTK widgets / `Rc` into background tasks.
- Keep drain callbacks small and bounded (update properties, don’t rebuild the tree).

### Overlay visibility gating (recommended for overlays)

For overlay UIs, DBus signals may continue even while the overlay is hidden. To keep the app responsive and avoid doing unnecessary GTK work:

- Only apply submenu/widget updates while the overlay is **visible**.
- When the overlay transitions **hidden → visible**, force a drain+repaint so the user never sees stale state on open.
- When the overlay becomes hidden, disable scheduling/draining repaints.

Practical pattern:
- Plugin exposes lightweight hooks like:
  - `on_overlay_shown()` → set `visible=true`, drain+repaint immediately
  - `on_overlay_hidden()` → set `visible=false`
- The app calls these hooks from the same code paths that show/hide the overlay window.

This keeps background DBus ingestion running (source of truth remains external), but makes GTK updates pay-for-play only when the UI is actually on screen.

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

#### DBus notifications (`org.freedesktop.Notifications`) capabilities

DBus implementation note / direction:

- Server-side implementation: use `zbus` (it provides a clearer, higher-level API for owning a well-known name, exporting an interface, and emitting signals).
- Client-side implementation: use `zbus` as well (single DBus stack; no `dbus` / `dbus-tokio`).
- Intent: keep DBus integrations **zbus-only** to avoid maintaining multiple DBus stacks long-term.

This app can optionally act as the notification server by owning the session-bus name `org.freedesktop.Notifications`.

##### Manual smoke test (DBus notifications)

Preconditions:

- Another notification daemon may already be running (GNOME Shell notifications, `dunst`, `mako`, etc.).
- Start `sacrebleui` normally. If the name is already owned, the app should attempt to **replace** the current owner during startup.
  - If replacement succeeds, `sacrebleui` becomes the notification server.
  - If replacement fails, startup should fail (and the app exits), rather than running without owning `org.freedesktop.Notifications`.

1) Verify sacrebleui owns `org.freedesktop.Notifications`

In a separate terminal, run:

- `busctl --user status org.freedesktop.Notifications`

This should show an owner (a unique name like `:1.123`) and should not report “not found”.

2) Send a notification via libnotify (`notify-send`)

Send a simple notification:

- `notify-send "sacrebleui smoke test" "Hello from notify-send"`

You should see it appear in the notifications UI.

3) Send a notification with markup (body-markup)

- `notify-send "Markup test" "<b>bold</b> <i>italic</i> <span foreground='red'>red</span>"`

The body should render with markup in the UI.

4) Send a notification with an action, and verify DBus signals

Start a DBus monitor for signals:

- `dbus-monitor --session "type='signal',interface='org.freedesktop.Notifications'"`

In another terminal, send a notification with an action:

- `notify-send --action=default=Open "Action test" "Click the action button"`

Now click the action button in the UI. `dbus-monitor` should show:

- an `ActionInvoked` signal containing the notification id and the action key (e.g. `"default"`), and
- a `NotificationClosed` signal after the action (per policy: close after action click).

5) Verify CloseNotification from the client side

This is easiest if you capture the id from `dbus-monitor` output (it will include the id in the signals).
Once you have an id, call CloseNotification manually:

- `gdbus call --session --dest org.freedesktop.Notifications --object-path /org/freedesktop/Notifications --method org.freedesktop.Notifications.CloseNotification <ID>`

The notification should be removed from the UI and `dbus-monitor` should show `NotificationClosed` with reason “closed by call”.

Notes:

- Replacement semantics (`replaces_id`) are client-driven. If you test with a client that sets `replaces_id`, the app should remove the old notification and create a new one.
- Persistence is not supported (in-memory only), so restarting the app clears notifications.

Operational policy:

- If `org.freedesktop.Notifications` is already owned by another process, the app should attempt to **replace** the owner during startup.
  - If it cannot acquire the name, the app should still **fail during startup** (exit the entire app).

Capabilities we support / intend to advertise via `GetCapabilities` (must match actual UI behavior):

- `actions` – supported (notification buttons).
- `body` – supported (notification body text).
- `body-markup` – supported (render markup in GTK).

Capabilities that are intentionally unimplemented / not advertised (documented so we don’t over-promise to clients):

- Persistence (`persistence`) – not supported (notifications are in-memory only).
- Desktop-entry / icon resolution via desktop files – not supported (see “app icon lookup” limitation above).
- Any additional capabilities not explicitly listed above should be treated as unsupported unless implemented and documented here.

Behavioral notes (must remain consistent):

- `Notify` returns DBus-generated notification IDs.
- `replaces_id`: create a new notification and remove the old one.
- Clicking an action in the UI emits `ActionInvoked` and then closes the notification (also emitting `NotificationClosed` with the appropriate reason).
- Dismissing a notification in the UI emits `NotificationClosed` (reason = dismissed by user).
- `CloseNotification` from clients is supported and results in removal + `NotificationClosed`.

### Feature toggles (still evolving)

- The codebase is still moving toward:
  - `FeatureToggle` owning `FeatureSpec` (no lifetimes),
  - `Plugin` without `Send + Sync` bounds.

Future contributors should:

- Preserve the separation between plugins, specs, models, and any future event bus.
- Prefer explicit, minimal coupling between plugins and UI composition.
- Be careful when introducing any new `Send + Sync` bounds that might conflict with GTK or `Rc`‑based structures.