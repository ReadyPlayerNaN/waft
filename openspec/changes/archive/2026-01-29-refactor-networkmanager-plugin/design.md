## Context

The NetworkManager plugin (`src/features/networkmanager/mod.rs`) currently implements all adapter management logic in a single 775-line file. This includes:
- Plugin initialization and device discovery
- UI widget creation for Ethernet and WiFi adapters
- Event handlers with deeply nested callbacks
- D-Bus async operations using thread spawning + channel polling
- State synchronization with the NetworkStore

The existing architecture has:
- **Store layer**: `NetworkStore` with `NetworkState` and `NetworkOp` - works well, no changes needed
- **Menu widgets**: `EthernetMenuWidget`, `WiFiMenuWidget` - presentational components that work correctly
- **Toggle components**: `FeatureToggleExpandableWidget` (shared), `WiFiToggleWidget` (simple presentational)
- **D-Bus layer**: `dbus.rs` with nmrs integration helpers

The problem is in `mod.rs` which couples everything together with deep nesting (callbacks within callbacks, match arms with 50+ lines, glib timeout handlers inline).

Current constraints:
- GTK4 + Relm4 patterns (Rc<RefCell<>>, widget cloning, glib main loop)
- nmrs for NetworkManager operations (async/await on separate thread)
- Store-driven architecture (emit ops, react to state changes)
- No breaking changes to public Plugin trait interface

## Goals / Non-Goals

**Goals:**
- Separate adapter-specific UI logic into dedicated widget modules
- Create consistent toggle components (add `WiredToggleWidget` matching `WiFiToggleWidget`)
- Reduce `mod.rs` to <200 lines as plugin coordinator only
- Eliminate deep nesting through extraction
- Maintain all existing functionality and user-facing behavior
- Keep the same store operations and state structure

**Non-Goals:**
- Changing the NetworkStore design or state structure
- Rewriting menu widgets (EthernetMenuWidget, WiFiMenuWidget)
- Changing the Plugin trait interface
- Adding new features or changing behavior
- Migrating away from the thread + channel async pattern (future work)

## Decisions

### Decision 1: Adapter Widget Pattern

**Choice:** Create `WiredAdapterWidget` and `WiFiAdapterWidget` as coordinator widgets that own:
- The toggle component (FeatureToggleExpandableWidget or simple toggle)
- The menu widget (EthernetMenuWidget / WiFiMenuWidget)
- Event handlers (toggle events, menu events, expand callbacks)
- State synchronization logic (store operations → UI updates)

**Rationale:**
- Each adapter type has distinct behavior (WiFi scanning, Ethernet carrier detection)
- Encapsulation reduces cognitive load - one adapter widget contains all logic for that adapter type
- Testing becomes easier - can test adapter widgets in isolation
- Follows single responsibility principle

**Alternatives considered:**
- Generic `AdapterWidget<T>` with type parameters - rejected because WiFi and Ethernet have fundamentally different behavior (scanning, carrier, etc.)
- Keep everything in mod.rs but split into functions - rejected because it doesn't address coupling, just moves code around

**Structure:**
```rust
// WiredAdapterWidget
pub struct WiredAdapterWidget {
    path: String,
    store: Arc<NetworkStore>,
    nm: Option<NetworkManager>,
    dbus: Arc<DbusHandle>,
    toggle: FeatureToggleExpandableWidget,
    menu: EthernetMenuWidget,
}

impl WiredAdapterWidget {
    pub fn new(adapter: &EthernetAdapterState, store, nm, dbus, menu_store) -> Self
    pub fn widget(&self) -> Arc<WidgetFeatureToggle>
    fn setup_toggle_handlers(&self)
    fn setup_expand_callback(&self)
    fn handle_toggle_event(&self, event: FeatureToggleExpandableOutput)
    fn handle_expand(&self, expanded: bool)
    fn sync_state(&self, state: &EthernetAdapterState)
}
```

### Decision 2: Toggle Component Consistency

**Choice:** Create `WiredToggleWidget` as a simple presentational component matching `WiFiToggleWidget`, BUT use `FeatureToggleExpandableWidget` as the actual component in `WiredAdapterWidget` for now.

**Rationale:**
- Current `WiFiToggleWidget` exists but is not used in the current implementation (it uses `FeatureToggleExpandableWidget` instead)
- `FeatureToggleExpandableWidget` is more feature-complete (expandable, menu integration)
- Creating `WiredToggleWidget` maintains pattern consistency for future refactoring
- Don't block this refactoring on a larger toggle widget migration

**Decision:** Create `WiredToggleWidget` with the same API as `WiFiToggleWidget`, but mark it as unused for now. Both adapter widgets will use `FeatureToggleExpandableWidget`.

### Decision 3: Async Pattern Handling

**Choice:** Keep the existing thread + channel + glib::timeout_add_local pattern for async D-Bus operations.

**Rationale:**
- GTK widgets must be updated on the main thread
- nmrs operations are async and need a tokio runtime
- Current pattern works: spawn thread with tokio runtime, send results via mpsc channel, poll in glib main loop
- Changing this is out of scope (complex, risky, no clear benefit for this refactoring)
- Extract these patterns into adapter widgets to reduce duplication

**Alternative considered:**
- Use relm4 async commands - rejected as too invasive, would require restructuring message passing

### Decision 4: Event Handler Extraction

**Choice:** Move all event handlers into adapter widget methods. Use closures to capture widget reference and call methods.

**Pattern:**
```rust
// In WiredAdapterWidget::setup_toggle_handlers
let widget_clone = /* clone adapter widget reference */;
toggle.connect_output(move |event| {
    widget_clone.handle_toggle_event(event);
});
```

**Rationale:**
- Reduces nesting by moving handler logic into named methods
- Makes event flow explicit (toggle event → handler method)
- Easier to test and reason about

### Decision 5: State Synchronization Strategy

**Choice:** Each adapter widget subscribes to store changes and updates its UI components.

**Pattern:**
```rust
// In WiredAdapterWidget::new or init
let widget_clone = /* clone adapter widget reference */;
store.subscribe(move |state: &NetworkState| {
    if let Some(adapter) = state.ethernet_adapters.get(&widget_clone.path) {
        widget_clone.sync_state(adapter);
    }
});
```

**Rationale:**
- Reactive: UI automatically updates when state changes
- Centralizes UI update logic in sync_state method
- Adapter widget owns the relationship between state and UI

**Alternative considered:**
- Update UI directly in event handlers - rejected because it couples actions to UI updates, makes it hard to react to external state changes

### Decision 6: mod.rs Responsibilities

**Choice:** Reduce `mod.rs` to:
- Plugin trait implementation (id, init, create_elements, get_feature_toggles)
- Device discovery during init (enumerate devices, emit AddAdapter ops)
- Widget registration (create adapter widgets, collect toggles)

**What moves OUT:**
- All toggle event handlers → adapter widgets
- All menu event handlers → adapter widgets
- All expand callbacks → adapter widgets
- All state synchronization → adapter widgets
- Inline D-Bus operations → adapter widgets (still call dbus.rs helpers)

**Result:** mod.rs becomes ~150-180 lines, primarily structural code.

## Risks / Trade-offs

### [Risk] Increased file count and indirection
**Mitigation:** This is acceptable - the alternative is a monolithic unmaintainable file. The cognitive load of understanding one adapter widget is much lower than understanding 775 lines of interleaved logic.

### [Risk] Duplication between WiredAdapterWidget and WiFiAdapterWidget
**Mitigation:** Some duplication is acceptable since they have fundamentally different behavior. Extract truly common patterns (thread+channel polling) into helper functions in dbus.rs if needed in future work.

### [Risk] Ownership and lifetime complexity with Rc<RefCell<>>
**Mitigation:** Follow existing GTK4/Relm4 patterns. Use strong references in closures, rely on GTK widget lifecycle. Document ownership in struct comments.

### [Risk] Breaking existing functionality during extraction
**Mitigation:** Extract incrementally, test after each step. Keep existing behavior identical. Run manual tests for each adapter type.

### [Risk] WiFiToggleWidget and WiredToggleWidget are created but unused
**Trade-off:** Accept this for now. They establish a pattern for future work. Adapter widgets will use FeatureToggleExpandableWidget until a broader toggle refactoring happens.

## Migration Plan

This is a refactoring with no external API changes. No deployment or rollback considerations.

**Implementation order:**
1. Create `WiredToggleWidget` (matches `WiFiToggleWidget` API)
2. Create `WiredAdapterWidget` (extract ethernet logic from mod.rs)
3. Create `WiFiAdapterWidget` (extract WiFi logic from mod.rs)
4. Update `mod.rs` to use new adapter widgets
5. Remove old code from mod.rs
6. Verify all functionality works

## Open Questions

- **Q:** Should VPN support be refactored at the same time?
  **A:** Only if VPN code exists in mod.rs with the same coupling issues. If VPN is minimal or well-structured, leave it for now.

- **Q:** Should we extract the thread+channel polling pattern into a helper?
  **A:** Not in this change. It's a nice-to-have but adds scope. Consider in future work if duplication becomes painful.

- **Q:** Do menu widgets need any changes?
  **A:** Only if the adapter widget API requires it. Prefer keeping menu widgets unchanged.
