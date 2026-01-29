## Context

The `src/ui/icon.rs` module provides icon resolution for the entire UI but has no test coverage. The project uses separate test files (e.g., `menu_state_tests.rs`) rather than inline `#[cfg(test)]` modules within the same file.

## Goals / Non-Goals

**Goals:**
- Verify `Icon::from_str` path classification logic
- Verify `resolve_themed_icon` fallback sequence
- Follow existing project test patterns
- Support headless CI environments

**Non-Goals:**
- Testing `IconWidget` (requires full GTK widget initialization)
- Testing `try_apply_icon` private method (covered indirectly)
- Mocking GTK - use real GTK with xvfb in CI

## Decisions

### Decision 1: Test File Location

Create `src/ui/icon_tests.rs` following the project's pattern (e.g., `menu_state_tests.rs`). Add `#[cfg(test)] mod icon_tests;` to `src/ui/icon.rs`.

**Rationale:** Consistency with existing test organization.

### Decision 2: GTK Dependency in Tests

Use real GTK4 APIs in tests rather than mocking. Tests requiring display/theme will gracefully fail if GTK is unavailable.

**Rationale:**
- Mocking GTK IconTheme is complex and brittle
- `xvfb-run` provides headless testing in CI
- Real GTK tests catch actual integration issues

### Decision 3: Test Data Strategy

For `Icon::from_str`: Use simple string literals covering edge cases.

For `resolve_themed_icon`: Use standard Adwaita icon names (e.g., "dialog-information") that exist in most GTK environments.

**Rationale:** Standard icons maximize test reliability across environments.
