## Why

The codebase contains 2,384 lines across 7 DBus modules with significant duplication and zero test coverage. Common patterns like property getting, value extraction, and PropertiesChanged listening are reimplemented in each feature module. Comments are verbose and sometimes outdated. Consolidating shared patterns into reusable helpers will reduce code size, improve maintainability, and make testing easier.

## What Changes

- Add comprehensive test coverage for all DBus modules (7 files)
- Extract common DBus patterns into reusable helpers in `src/dbus.rs`:
  - Typed property getters with defaults
  - Value extraction utilities (consolidate `owned_value_to_*` helpers)
  - GetAll property fetching with typed extraction
  - PropertiesChanged signal listener wrapper
- Update comments: remove redundant explanations, fix outdated ones
- Refactor feature DBus modules to use new shared helpers
- Document reduction opportunities where code can be simplified

## Capabilities

### New Capabilities
- `dbus-core-testing`: Tests for `src/dbus.rs` core functionality (property get/set, signal listening, value conversion)
- `dbus-feature-testing`: Tests for feature module DBus code (darkman, battery, bluetooth, audio, networkmanager, agenda)
- `dbus-shared-helpers`: Reusable helper functions for common DBus patterns (typed getters, GetAll wrapper, PropertiesChanged listener, value extractors)
- `dbus-comment-quality`: Clear, minimal, up-to-date comments across all DBus modules

### Modified Capabilities
<!-- No existing capabilities being modified -->

## Impact

**Files affected:**
- `src/dbus.rs`: Add shared helpers, add tests (net: likely +200 lines for helpers/tests, -50 from comment cleanup)
- `src/features/darkman/dbus.rs`: Use shared helpers, add tests (25 lines → ~40 lines)
- `src/features/battery/dbus.rs`: Use shared helpers, add tests (129 lines → ~100 lines estimated)
- `src/features/bluetooth/dbus.rs`: Remove duplicated helpers, use shared ones, add tests (237 lines → ~180 lines estimated)
- `src/features/audio/dbus.rs`: Add tests, document (962 lines, mostly pactl parsing - limited DBus consolidation opportunity)
- `src/features/networkmanager/dbus.rs`: Use shared helpers, add tests (391 lines → ~300 lines estimated)
- `src/features/agenda/dbus.rs`: Use shared helpers, add tests (356 lines → ~280 lines estimated)

**Expected outcome:**
- ~300-400 lines of new tests
- ~200-300 lines reduction from DRYing common patterns
- Net: similar or slightly more total lines, but much better quality/coverage
- All DBus functionality will be tested
- Future DBus integrations will be easier to implement using shared helpers
