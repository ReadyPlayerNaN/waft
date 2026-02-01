## Context

The application currently lacks a compositor-agnostic way to display and switch keyboard layouts from the overlay interface. Users must rely on compositor-specific tools (e.g., Sway's bindings, Hyprland's widgets) or system settings to manage keyboard layouts.

### Current State
- The plugin system uses a registration-based architecture (see `plugin.rs`, `plugin_registry.rs`)
- Plugins implement the `Plugin` trait with lifecycle methods: `init()`, `create_elements()`
- Widgets are registered to slots (e.g., `Slot::Header`) via `WidgetRegistrar`
- The systemd_actions feature demonstrates the pattern: D-Bus client + widget + plugin registration
- D-Bus integration is handled via `DbusHandle` which provides async access to system and session buses

### Constraints
- Must work across different Wayland compositors (Sway, Hyprland, etc.)
- Must use D-Bus for compositor-agnostic layout management
- Must follow existing plugin architecture patterns
- Must handle D-Bus unavailability gracefully (don't crash, show fallback UI)
- Must use async/await with tokio runtime for D-Bus calls
- Must bridge glib (GTK) and tokio runtimes safely using `crate::runtime::spawn_on_tokio`

### Stakeholders
- End users who need quick keyboard layout switching
- Wayland compositor users (cross-compositor compatibility)
- Plugin developers (establishing patterns for future D-Bus widgets)

## Goals / Non-Goals

**Goals:**
- Display current keyboard layout abbreviation in the overlay header
- Cycle through available keyboard layouts on button click
- React to external layout changes (e.g., from compositor keybindings)
- Provide compositor-agnostic implementation via D-Bus
- Include comprehensive tests (unit, integration, widget)
- Document D-Bus requirements and compositor compatibility in README

**Non-Goals:**
- Layout configuration UI (users configure via system settings or XKB config files)
- Visual layout indicator beyond text abbreviation (no flag icons, custom graphics)
- Multi-display keyboard layout management (system-wide layout only)
- Layout switching via dropdown/menu (only click-to-cycle for simplicity)
- Support for non-XKB keyboard layout systems

## Decisions

### Decision 1: Use org.freedesktop.locale1 as primary D-Bus interface

**Rationale:**
- `org.freedesktop.locale1` is part of systemd's localed service, available on most modern Linux systems
- Provides `X11Layout` property for querying configured layouts
- Provides `SetX11Keyboard` method for changing layouts
- Compositor-agnostic (doesn't depend on Sway, Hyprland, etc.)

**Alternatives considered:**
- **Compositor-specific D-Bus interfaces** (e.g., Sway IPC, Hyprland sockets): Would require compositor detection and multiple implementations, violating compositor-agnostic requirement
- **X11-style setxkbmap command-line tool**: Not a D-Bus solution, harder to get change notifications, not aligned with systemd integration
- **Direct XKB library integration**: Lower-level than needed, no change notifications, more complexity

**Trade-offs:**
- Requires systemd-localed running (acceptable for target systems)
- Layout changes via localed may require PolicyKit authorization depending on system configuration
- If localed is unavailable, widget shows fallback state (acceptable degradation)

### Decision 2: Follow systemd_actions plugin architecture pattern

**Rationale:**
- Consistency with existing codebase patterns
- Proven architecture for D-Bus + widget integration
- Clear separation of concerns: `mod.rs` (plugin), `dbus.rs` (D-Bus client), `widget.rs` (GTK widget)
- Established error handling patterns (graceful D-Bus unavailability)

**Structure:**
```
src/features/keyboard_layout/
├── mod.rs           # Plugin implementation, lifecycle, widget registration
├── dbus.rs          # KeyboardLayoutClient for D-Bus interaction
├── widget.rs        # KeyboardLayoutWidget (GTK button)
└── README.md        # Documentation for setup and D-Bus requirements
```

**Alternatives considered:**
- **Single-file plugin**: Less organized, harder to test D-Bus logic separately
- **Shared D-Bus module**: Keyboard layout logic is specific enough to warrant its own client

### Decision 3: Display layout as uppercase abbreviation (e.g., "US", "DE")

**Rationale:**
- Compact and readable in header button
- Follows common conventions (system trays, status bars)
- Easy to parse from XKB layout string (e.g., "us,de,fr" → ["US", "DE", "FR"])

**Alternatives considered:**
- **Full layout names** (e.g., "English (US)", "German"): Too long for compact header button
- **Flag icons**: Requires asset management, not all layouts map to countries, accessibility concerns
- **Lowercase abbreviations**: Less visually distinct, less conventional

**Trade-offs:**
- Multi-variant layouts lose variant information (e.g., "us(dvorak)" shows as "US")
- Users must know their layout abbreviations (acceptable, shown in system settings)

### Decision 4: Subscribe to PropertiesChanged signals for external layout changes

**Rationale:**
- Allows widget to react when layout is changed via compositor keybindings or other tools
- D-Bus PropertiesChanged signals are standard for property monitoring
- Ensures UI stays in sync with system state

**Implementation:**
- Subscribe to `org.freedesktop.DBus.Properties.PropertiesChanged` for `X11Layout` property
- Update button label when signal is received
- Use glib::spawn_future_local to invoke callback in GTK main thread

**Alternatives considered:**
- **Polling X11Layout property periodically**: Wasteful, introduces latency, not event-driven
- **No external change detection**: Widget would show stale state when layout changes externally

### Decision 5: Cycle layouts in configured order (no reverse cycling)

**Rationale:**
- Simple UX: single click cycles forward through layouts
- Matches common layout switching behavior (e.g., Win+Space on Windows, Alt+Shift on GNOME)
- Reduces complexity (no need for separate forward/reverse buttons)

**Implementation:**
- Query available layouts from `X11Layout` property (comma-separated list)
- Find current layout index, increment with wrap-around
- Call `SetX11Keyboard` with next layout

**Alternatives considered:**
- **Forward + reverse buttons**: More complex UI, takes more header space
- **Dropdown menu**: More clicks required, inconsistent with click-to-cycle pattern

**Trade-offs:**
- Users with many configured layouts must click multiple times to reach specific layout
- Acceptable for typical 2-3 layout configurations

### Decision 6: Widget weight 95 for header positioning

**Rationale:**
- Positions keyboard layout button to the right side of header
- Places it before systemd_actions widgets (weight 100, 101)
- Logical grouping: layout control → session/power actions

**Alternatives considered:**
- **Weight > 101**: Would place after power button, less intuitive (layout is more frequently changed)
- **Weight < 95**: Would place further left, away from other quick-access buttons

### Decision 7: Mock D-Bus client for unit tests

**Rationale:**
- Allows testing layout parsing, cycling logic, error handling without D-Bus dependency
- Faster test execution (no D-Bus connection required)
- Follows existing patterns in codebase (see `dbus_tests.rs`)

**Implementation:**
- Define `KeyboardLayoutClient` trait with methods: `get_current_layout()`, `get_available_layouts()`, `set_layout()`, `subscribe_layout_changes()`
- Real implementation: `LocaledKeyboardLayoutClient` (uses D-Bus)
- Mock implementation: `MockKeyboardLayoutClient` (returns configurable test data)

**Alternatives considered:**
- **No trait abstraction, only integration tests**: Slower test suite, harder to test edge cases
- **Use existing systemd-localed service for tests**: Fragile (depends on system state), may require root/PolicyKit

### Decision 8: Include README.md in plugin directory

**Rationale:**
- Documents D-Bus service requirements (systemd-localed)
- Provides setup instructions for users/distributors
- Explains compositor compatibility and fallback behavior
- Establishes documentation pattern for D-Bus-dependent plugins

**README sections:**
- Overview and features
- D-Bus dependencies (org.freedesktop.locale1)
- Compositor compatibility notes
- Troubleshooting (what to check if plugin doesn't work)
- Testing instructions

## Risks / Trade-offs

### [Risk] systemd-localed not available on target system
**Mitigation:**
- Graceful degradation: Plugin initialization succeeds even if D-Bus fails
- Widget shows fallback label ("??") to indicate unavailability
- Log warning messages for debugging
- README documents localed requirement and how to check availability

### [Risk] Layout changes require PolicyKit authorization
**Mitigation:**
- Use `interactive: true` flag in SetX11Keyboard to allow PolicyKit prompts
- Show error dialog if authorization fails (similar to systemd_actions pattern)
- Document PolicyKit requirements in README
- Log authorization errors for debugging

### [Risk] XKB layout string parsing complexity
**Mitigation:**
- Start with simple comma-separated parsing (e.g., "us,de,fr")
- Handle edge cases: empty string, single layout, malformed input
- Unit tests for all parsing scenarios
- Log parsing failures for debugging
- Ignore variant information for simplicity (e.g., "us(dvorak)" → "US")

### [Risk] Race conditions in async D-Bus signal handling
**Mitigation:**
- Use `Arc<Mutex<Option<KeyboardLayoutClient>>>` for shared access (follows systemd_actions pattern)
- Use `crate::runtime::spawn_on_tokio` for all D-Bus calls (tokio runtime)
- Use `glib::spawn_future_local` for GTK UI updates (glib runtime)
- Avoid holding locks across await points
- Follow async-runtime-bridge patterns documented in skills

### [Risk] Layout changes not detected when triggered externally
**Mitigation:**
- Subscribe to PropertiesChanged signals during widget initialization
- Test signal subscription with external tools (e.g., `localectl set-x11-keymap`)
- Handle signal subscription failures gracefully (fallback to manual refresh)
- Document signal behavior in README

### [Risk] Performance impact of D-Bus calls on UI thread
**Mitigation:**
- All D-Bus calls are async (don't block GTK main thread)
- Use `spawn_on_tokio` to offload D-Bus work to tokio runtime
- Button click handler spawns async task immediately, doesn't wait for result
- Widget remains responsive during D-Bus operations

### [Trade-off] Limited layout information in compact button
**Acceptance:**
- Button shows only current layout abbreviation (e.g., "US")
- No variant information (e.g., can't distinguish "us" vs "us(dvorak)" in button)
- Users can verify full layout in system settings if needed
- Compact display is necessary for header space constraints

### [Trade-off] No visual feedback during layout switch
**Acceptance:**
- Button label updates when layout change succeeds
- No loading spinner or transition animation (keeps implementation simple)
- D-Bus calls are typically fast (<100ms) so instant feedback is acceptable
- Error dialog shown only on failure

## Migration Plan

This is a new plugin, no migration required.

**Deployment steps:**
1. Add plugin to `src/features/mod.rs` module list
2. Add plugin to plugin registry initialization in `app.rs`
3. Test with systemd-localed available and unavailable
4. Document D-Bus requirements in README
5. Verify compositor compatibility (Sway, Hyprland, others)

**Rollback strategy:**
- Remove plugin from registry initialization
- Plugin is self-contained, removal doesn't affect other features

## Open Questions

### Q1: Should we support fallback to compositor-specific D-Bus interfaces?

**Context:** If org.freedesktop.locale1 is unavailable, should we try compositor-specific interfaces (e.g., Sway IPC)?

**Decision needed:** During implementation phase

**Recommendation:** Start with localed-only, add compositor fallbacks only if users report issues

### Q2: Should layout switching trigger confirmation dialog?

**Context:** Similar to how systemd_actions may show PolicyKit prompts, should we confirm layout switches?

**Decision needed:** During implementation phase

**Recommendation:** No confirmation for layout switching (low-risk operation, easily reversible by cycling again)

### Q3: Should we cache layout list or query on every click?

**Context:** `X11Layout` property could be cached in memory or queried fresh on each cycle.

**Decision needed:** During implementation phase

**Recommendation:** Cache layout list, refresh on PropertiesChanged signal (reduces D-Bus calls, improves responsiveness)

### Q4: Should we display full layout name as tooltip?

**Context:** Button shows abbreviation ("US"), tooltip could show full name ("English (US)").

**Decision needed:** During implementation phase

**Recommendation:** Add accessible tooltip with full layout name for better accessibility
