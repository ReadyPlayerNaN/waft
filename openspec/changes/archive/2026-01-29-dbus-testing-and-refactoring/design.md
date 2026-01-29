## Context

The application has 7 DBus modules totaling 2,384 lines with no test coverage. Each feature module (darkman, battery, bluetooth, audio, networkmanager, agenda) independently implements DBus interactions, leading to:

- Duplicated value extraction helpers (`owned_value_to_string`, `owned_value_to_bool` in bluetooth)
- Inconsistent property getting patterns (some use `DbusHandle`, others create raw `Connection`)
- Manual proxy creation and GetAll property parsing repeated across modules
- PropertiesChanged listening implemented only in battery module, but needed elsewhere
- Verbose, sometimes outdated comments explaining basic DBus concepts

The core `src/dbus.rs` provides basic primitives (property get/set, signal listening) but lacks higher-level helpers for common patterns.

**Testing challenges:**
- DBus code requires mock services or integration tests
- Async/tokio runtime adds complexity
- GTK integration means tests may need display availability

## Goals / Non-Goals

**Goals:**
- Add comprehensive unit and integration tests for all DBus code
- Extract common patterns into reusable helpers in `src/dbus.rs`
- Reduce duplication across feature modules
- Improve comment quality: remove redundant, fix outdated
- Make future DBus integrations easier

**Non-Goals:**
- Not changing DBus API behavior (non-breaking refactor only)
- Not mocking GTK (use real GTK with xvfb if needed, like icon tests)
- Not testing pactl parsing logic in audio module (large, separate concern)
- Not rewriting working code - only consolidate duplicated patterns

## Decisions

### Decision 1: Shared Helper Location

Add shared helpers to `src/dbus.rs` rather than creating a new module.

**Rationale:**
- `src/dbus.rs` already exports `DbusHandle` - natural place for extensions
- Avoids introducing new module hierarchy
- Keeps all DBus primitives in one place

**New helpers:**
- `DbusHandle::get_typed_property<T>` - generic property getter with type conversion
- `DbusHandle::get_all_properties` - GetAll wrapper returning HashMap
- `DbusHandle::listen_properties_changed` - PropertiesChanged signal helper
- Module-level `owned_value_to_bool`, `owned_value_to_u32`, etc. (consolidate from bluetooth)

### Decision 2: Testing Strategy

Use three-tier approach:
1. **Unit tests**: Value conversion helpers (no DBus required)
2. **Mock integration tests**: Test with in-memory mock DBus server (using zbus test utilities)
3. **Documentation tests**: Show usage patterns in doc comments

**Rationale:**
- Unit tests are fast, cover helpers
- Mock integration tests verify DBus protocol interactions without external dependencies
- zbus provides `dbus_interface` macro for creating test services
- Documentation tests serve as examples

**Not using:**
- Real system services (flaky in CI)
- Heavy mocking frameworks (zbus test utilities sufficient)

### Decision 3: Refactoring Approach

Refactor incrementally by module, not all at once:
1. Add helpers to `src/dbus.rs`
2. Add tests for core helpers
3. Refactor one feature module at a time
4. Add tests for each refactored module

**Rationale:**
- Smaller, reviewable changes
- Can pause/resume work easily
- Reduces risk of breaking multiple features
- Allows testing each module's refactor independently

### Decision 4: Value Extraction API

Consolidate value extractors as standalone functions, not methods:
```rust
pub fn owned_value_to_bool(v: OwnedValue) -> Option<bool>
pub fn owned_value_to_string(v: OwnedValue) -> Option<String>
pub fn owned_value_to_u32(v: OwnedValue) -> Option<u32>
```

Not as `DbusHandle` methods because they don't need connection state.

**Alternatives considered:**
- Make them methods on `OwnedValue` via trait - rejected (can't extend foreign type)
- Put in separate `conversions` module - rejected (overkill for 3-4 functions)

### Decision 5: Comment Guidelines

Apply these rules when updating comments:
- Remove comments explaining basic Rust or DBus concepts (trust reader knowledge)
- Keep comments explaining non-obvious design decisions or workarounds
- Update outdated comments (e.g., "TODO" that's been done)
- Use doc comments (`///`) for public API, regular comments for internal notes
- Prefer self-documenting code over comments where possible

**Example cleanup:**
```rust
// BEFORE:
// Read a DBus property as a `String` (best-effort).
//
// Notes:
// - This uses `org.freedesktop.DBus.Properties.Get`.
// - `destination` here is the **service name** you're talking to (e.g. `nl.whynothugo.darkman`).
// - Returns `Ok(None)` if the property exists but is not a string.

// AFTER:
/// Get a string property via org.freedesktop.DBus.Properties.Get.
/// Returns None if property exists but isn't a string.
```

### Decision 6: NetworkManager Connection Inconsistency

The networkmanager module creates raw `Connection::system()` instead of using `DbusHandle`.

**Decision:** Standardize on `DbusHandle::connect_system()` for consistency.

**Rationale:**
- Consistent API across all modules
- Easier to mock for testing
- Connection pooling/reuse handled by `DbusHandle`

**Migration:** Update networkmanager to accept `&DbusHandle` parameter instead of creating connections.

## Risks / Trade-offs

**[Risk]** Tests may be flaky if they depend on external DBus services
→ **Mitigation:** Use mock DBus servers via zbus test utilities, not real system services

**[Risk]** Refactoring could introduce subtle bugs in DBus interactions
→ **Mitigation:** Add tests before refactoring each module, verify all existing functionality still works

**[Risk]** Test setup complexity (tokio runtime, async, mock services)
→ **Mitigation:** Create helper functions for common test setup patterns (we learned this from icon tests)

**[Risk]** Large change touching many files could be hard to review
→ **Mitigation:** Break into incremental commits: helpers first, then one module at a time

**[Trade-off]** Adding helpers increases `src/dbus.rs` size
→ **Accepted:** Core module size increase is worth DRYing feature modules (net reduction overall)

**[Trade-off]** Audio module has limited consolidation opportunity (pactl-based, not pure DBus)
→ **Accepted:** Focus on testing audio module, less refactoring needed there

## Open Questions

None - ready to proceed with implementation.
