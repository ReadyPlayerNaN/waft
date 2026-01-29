## Context

The networkmanager plugin currently implements its own D-Bus communication layer in `src/features/networkmanager/dbus.rs`, manually constructing D-Bus method calls using `zbus` and parsing property values. This approach:
- Duplicates ~600 lines of boilerplate D-Bus code
- Requires manual type conversions from `OwnedValue` to Rust types
- Lacks compile-time guarantees for D-Bus interface correctness
- Makes future NetworkManager features harder to add

**This migration uses nmrs as the primary API** - the custom D-Bus implementation is largely replaced with `nmrs`. D-Bus is retained only for features nmrs doesn't support:
- Link speed queries (nmrs doesn't expose)
- Saved WiFi connection profile lookup (nmrs doesn't expose)
- WiFi connection activation with saved credentials (nmrs requires credentials)

The `nmrs` crate (v2.0.0) is a mature, battle-tested library that provides:
- Type-safe NetworkManager API with proper Rust types (`DeviceState`, `AccessPoint`, `Ip4Config`, etc.)
- Automatic D-Bus serialization/deserialization
- Signal-based monitoring for device and network changes
- Comprehensive coverage of NetworkManager interfaces
- Active maintenance and community support

**Code savings: ~450 lines** - Original `dbus.rs` was 772 lines; new implementation is 318 lines.

**Constraints:**
- Must maintain existing UI behavior (no user-visible changes)
- Must preserve async/await patterns for D-Bus calls (GTK main loop integration)
- Must work with existing `DbusHandle` wrapper (shared connection with other plugins)
- Cannot break existing store operations consumed by UI components

**Stakeholders:**
- End users: No visible changes, same functionality
- Developers: Simpler, more maintainable NetworkManager integration
- Future contributors: Easier to add NetworkManager features

## Goals / Non-Goals

**Goals:**
- Replace manual D-Bus calls with nmrs API throughout `dbus.rs`
- Maintain 100% functional equivalence with current implementation
- Reduce code complexity and maintenance burden
- Improve type safety with nmrs type system
- Keep existing store types and operations unchanged (UI compatibility)

**Non-Goals:**
- Changing the UI or user-facing behavior
- Refactoring the store architecture or state management
- Adding new NetworkManager features beyond current scope
- Modifying other plugins or the DbusHandle abstraction
- Changing async patterns (still using tokio runtime in separate threads)

## Decisions

### Decision 1: Keep internal store types, create adapter layer

**Choice:** Maintain existing store types (`EthernetAdapterState`, `WiFiAdapterState`, `AccessPointState`, etc.) and create adapters to convert nmrs types to store types.

**Rationale:**
- Minimizes impact on store.rs, ethernet_menu.rs, wifi_menu.rs, and mod.rs
- Allows incremental migration without breaking UI components
- Isolates nmrs integration to dbus.rs module
- Reduces risk of regression in UI behavior

**Alternative considered:** Directly use nmrs types in store
- **Rejected because:** Would require cascading changes through store, UI components, and event handlers. Higher risk of breaking existing behavior.

### Decision 2: Let nmrs manage its own D-Bus connection

**Choice:** Use nmrs's built-in connection management via `NetworkManager::new()` instead of sharing the `DbusHandle` connection.

**Rationale:**
- nmrs is designed to manage its own D-Bus connection and signal subscriptions
- nmrs does not provide a `from_connection()` API - it creates connections via `new()` or `with_config()`
- D-Bus connections are lightweight; having a separate connection for NetworkManager is acceptable
- This cleanly separates NetworkManager communication from other plugins
- Allows nmrs to manage its own subscription lifecycle for device/network monitoring

**Alternative considered:** Share DbusHandle connection with nmrs
- **Rejected because:** nmrs does not support injecting an external connection. The library is designed to own its D-Bus connection for proper signal subscription management.

**Note:** Other plugins (bluetooth, power, etc.) continue to use `DbusHandle`. Only the networkmanager plugin will use nmrs's separate connection.

### Decision 3: Incremental function-by-function migration

**Choice:** Replace each dbus.rs function one at a time with nmrs equivalent, maintaining the same function signatures initially.

**Rationale:**
- Reduces risk by allowing testing after each function migration
- Easier to identify regressions if they occur
- Can maintain existing tests during migration
- Allows partial rollback if issues found

**Alternative considered:** Rewrite entire dbus.rs module at once
- **Rejected because:** Higher risk, harder to test incrementally, difficult to bisect failures.

### Decision 4: Handle async patterns with nmrs

**Choice:** Continue using the current pattern of spawning threads with tokio runtime for nmrs async calls, polling results in glib main loop.

**Rationale:**
- nmrs methods are async (return futures)
- Matches existing pattern in mod.rs (lines 240-290, 445-455)
- Avoids mixing tokio runtime with glib main loop
- Keeps consistency with current architecture

**Alternative considered:** Use glib async integration for nmrs
- **Rejected because:** Would require rewriting all async handlers in mod.rs, increases scope significantly, higher risk.

### Decision 5: Mapping nmrs device types to internal types

**Choice:** Map nmrs device types using match statements in adapter functions:
- `nmrs::Device` → `DeviceInfo`
- `nmrs::AccessPoint` → `AccessPointState`
- `nmrs::Ip4Config` / `nmrs::Ip6Config` → `Ip4Config` / `Ip6Config`

**Rationale:**
- Explicit mapping makes type conversions visible and maintainable
- Allows adding debug logging during conversion
- Easy to adjust mapping logic if needed
- Preserves internal type naming conventions

**Alternative considered:** Use From/Into traits for automatic conversion
- **Rejected because:** Less explicit, harder to debug, adds trait complexity for minimal benefit.

### Decision 6: Virtual interface filtering strategy

**Choice:** Keep existing virtual interface filtering logic (checking prefixes like "docker", "veth", etc.) applied to nmrs device data.

**Rationale:**
- Current filtering works correctly
- nmrs provides device properties but doesn't do filtering itself
- Business logic (what to filter) should remain in our code
- Simple to apply same filter to nmrs Device objects

**Alternative considered:** Remove filtering and rely on nmrs
- **Rejected because:** nmrs doesn't provide this filtering, would show unwanted virtual interfaces to users.

## Risks / Trade-offs

### Risk: nmrs API differences from manual D-Bus calls
**Mitigation:**
- Read nmrs documentation carefully for each replaced function
- Test each migration step with real NetworkManager
- Keep existing integration tests to catch behavioral differences
- Reference nmrs examples and source code when unclear

### Risk: nmrs version updates breaking compatibility
**Mitigation:**
- Pin nmrs to 2.x in Cargo.toml (`nmrs = "2.0"`)
- Document which nmrs APIs we depend on
- Review nmrs changelog before updating versions
- Consider contributing upstream fixes if issues found

### Risk: Performance regression with nmrs
**Mitigation:**
- nmrs uses its own D-Bus connection, but D-Bus connections are lightweight
- Signal-based monitoring is more efficient than polling
- Monitor for any noticeable UI lag during testing
- Profile if concerns arise (unlikely given similar architecture)

### Risk: Incomplete nmrs API coverage
**Mitigation:**
- Review nmrs docs for all needed NetworkManager interfaces
- Fallback to manual zbus calls if nmrs lacks specific APIs
- Current survey shows nmrs covers all needed operations (devices, connections, WiFi, IP config)

### Trade-off: Additional dependency
- **Risk:** Adds nmrs (and its dependencies) to build
- **Benefit:** Removes ~600 lines of manual D-Bus code, reduces maintenance burden
- **Assessment:** Worth it - battle-tested library is more reliable than custom code

### Trade-off: Abstraction layer overhead
- **Risk:** Adapter functions add slight conversion overhead
- **Benefit:** Isolates nmrs changes, maintains UI compatibility
- **Assessment:** Worth it - conversions are trivial (copying fields), negligible performance impact

## Migration Plan

**Phase 1: Setup**
1. Add `nmrs = "2.0"` to Cargo.toml dependencies
2. Import nmrs in dbus.rs module
3. Create helper function to initialize `nmrs::NetworkManager::new()`
4. Verify compilation with nmrs added

**Phase 2: Core operations (availability, device enumeration)**
1. Replace `check_availability()` with nmrs API
2. Replace `get_all_devices()` with nmrs device enumeration
3. Replace `get_device_property()` generic with nmrs device property access
4. Test device detection with real NetworkManager

**Phase 3: Ethernet operations**
1. Replace `get_device_state()` with nmrs
2. Replace `get_device_active_connection()` with nmrs
3. Replace `get_wired_carrier()` with nmrs
4. Replace `activate_device()` / `disconnect_device()` with nmrs
5. Replace IP config queries (`get_ip4_config`, `get_ip6_config`, `get_link_speed`) with nmrs
6. Test ethernet device connection/disconnection

**Phase 4: WiFi operations**
1. Replace `get_wireless_enabled()` / `set_wireless_enabled()` with nmrs
2. Replace `request_scan()` with nmrs
3. Replace `get_access_points()` and access point property reading with nmrs
4. Replace `get_connections_for_ssid()` with nmrs
5. Replace `activate_connection()` with nmrs for WiFi
6. Test WiFi scanning and connection

**Phase 5: Cleanup**
1. Remove unused manual D-Bus helper functions
2. Remove unnecessary `zbus::zvariant::OwnedValue` imports
3. Update module documentation to reference nmrs
4. Run full integration tests

**Rollback strategy:**
- Git branch allows easy revert
- Function-by-function approach allows keeping working functions while fixing issues
- Can maintain dual implementation temporarily if needed (manual D-Bus as fallback)

## Open Questions

1. **Does nmrs 2.0 support all NetworkManager D-Bus interfaces we use?**
   - Need to verify: Settings.Connection.GetSettings, AccessPoint properties, IP4Config/IP6Config details
   - Resolution: Check nmrs documentation and examples before implementation

2. **How does nmrs handle D-Bus property changes and signals?**
   - Current implementation doesn't use signals, but future enhancement might need it
   - Resolution: Document for future reference, not needed for initial migration

3. **Should we update tests to use nmrs types or keep current mock approach?**
   - Current tests might use mock D-Bus responses
   - Resolution: Decide during implementation based on test architecture
