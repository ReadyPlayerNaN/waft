## Context

The main window currently builds its content once during startup by calling `PluginRegistry::get_widgets_for_slot()` and `get_all_feature_toggles()`. These methods iterate over all plugins, collect their widgets, and return static snapshots. Plugins store widgets internally and expose them via `Plugin::get_widgets()` and `Plugin::get_feature_toggles()`.

The codebase already has a well-established `PluginStore<Op, State>` pattern (src/store.rs) that provides subscription-based reactivity. The MenuStore uses this pattern to coordinate expandable menus across the UI.

## Goals / Non-Goals

**Goals:**
- Enable plugins to add/remove widgets at runtime without main window references
- Reuse the existing store/subscription pattern for consistency
- Minimize changes to existing plugin implementations
- Support both slot widgets (Header, Info, Controls) and feature toggles
- Maintain UI stability: only add/remove widgets that actually changed, reorder in-place when possible

**Non-Goals:**
- Real-time widget content updates (plugins already handle internal state reactivity)
- Plugin hot-loading/unloading at runtime

## Decisions

### 1. Extend PluginRegistry with store-like subscription

**Decision:** Add subscription support directly to `PluginRegistry` rather than creating a separate `WidgetStore`.

**Rationale:** The registry already holds all plugin references and widget access methods. Adding subscriptions here keeps the architecture simple and avoids another indirection layer. Plugins already interact with the registry during initialization.

**Alternatives considered:**
- Separate `WidgetStore` struct: Would duplicate widget tracking logic and require syncing between registry and store
- Per-slot stores: Overly complex for the use case; a single notification is sufficient since rebuilding UI sections is cheap

### 2. Pull-based notification model

**Decision:** Subscribers receive change notifications (no payload). They then call `get_widgets_for_slot()` or `get_all_feature_toggles()` to get current state.

**Rationale:** Matches the existing `PluginStore` pattern where `emit()` triggers notification and subscribers call `get_state()`. Keeps the API simple and avoids complex diff payloads.

**Alternatives considered:**
- Push widget diffs in notification: Complex to implement, requires tracking previous state, and GTK widget lifecycle makes diffing error-prone
- Event-sourced changes: Overkill; the UI simply needs to know "something changed"

### 3. Plugin-initiated registration via registry handle

**Decision:** Plugins receive an `Arc<WidgetRegistrar>` (a trait/handle to registry) during `create_elements()`. They call `registrar.register_widget(widget)` and `registrar.unregister_widget(id)` to manage their widgets.

**Rationale:** Decouples plugins from the full registry. Plugins don't need mutable registry access, just the ability to register/unregister their own widgets. The registrar internally notifies subscribers.

**Alternatives considered:**
- Plugins emit to a channel that registry listens to: Adds async complexity; widgets must be created on main thread anyway
- Keep `get_widgets()` but add `on_widgets_changed` callback: Inverts control awkwardly; plugins would need to store and call callbacks

### 4. Widget identity via stable IDs

**Decision:** Each `Widget` and `WidgetFeatureToggle` gets a unique `id: String` field (e.g., `"networkmanager:wifi:adapter-0"`). Unregistration uses this ID.

**Rationale:** GTK widgets can't be reliably compared by reference after being added to containers. String IDs are simple, debuggable, and allow plugins to manage their own namespace.

**Alternatives considered:**
- Use `Arc` pointer equality: Fragile with GTK's reference semantics
- Auto-generated UUIDs: Less meaningful for debugging; plugins can't predict IDs for unregistration

### 5. Diff-based container synchronization

**Decision:** Main window subscribes to registry. On notification, it compares current container children (by widget ID) against the new widget list and performs minimal DOM-style updates: remove only widgets no longer present, add only new widgets, reorder existing widgets in-place using `gtk::Box::reorder_child_after()`.

**Rationale:** Avoids unnecessary widget remounting which can cause visual flicker, reset internal widget state (focus, scroll position), and trigger unnecessary redraws. GTK4 boxes support efficient reordering via `reorder_child_after()`.

**Alternatives considered:**
- Full clear and rebuild: Simpler but causes flicker and remounts widgets unnecessarily
- Event-sourced add/remove tracking: Requires registry to track and emit granular changes; pull-based diffing is simpler

## Risks / Trade-offs

**[Diff algorithm complexity]** → Comparing current children to new list requires iterating both and tracking IDs. Keep the algorithm simple: O(n) iteration with a HashSet for lookups. GTK containers typically have <20 widgets.

**[Plugin must track its own widget IDs]** → Small burden on plugins, but keeps registry stateless about plugin internals. Provide helper macros or a base struct if this becomes repetitive.

**[Breaking change to Plugin trait]** → `create_elements()` signature changes to receive registrar. All plugins need updates, but the migration is mechanical.

**[Memory: old widgets not freed]** → When unregistering, ensure widgets are removed from containers and all `Arc` references are dropped. Document this requirement for plugins.

**[Widget ID must be accessible from GTK widget]** → To diff, we need to map container children back to IDs. Options: store ID in widget name (`widget.set_widget_name()`), or maintain a parallel `HashMap<WidgetId, gtk::Widget>` in the container wrapper.
