## Why

The icon resolution logic in `src/ui/icon.rs` has no test coverage. The `Icon::from_str` function classifies icon strings (themed vs file path) and `resolve_themed_icon` tries multiple name variants against the system theme. These functions are core to icon display across the UI (notifications, feature toggles, etc.) but lack verification. Adding tests will prevent regressions and document expected behavior.

## What Changes

- Add unit tests for `Icon::from_str` covering path detection rules
- Add tests for `resolve_themed_icon` covering fallback logic (exact, symbolic, lowercase variants)
- Create new test file `src/ui/icon_tests.rs` following project test patterns

## Capabilities

### New Capabilities
- `icon-classification-tests`: Tests verifying Icon::from_str correctly classifies strings as FilePath vs Themed based on path indicators
- `icon-theme-resolution-tests`: Tests verifying resolve_themed_icon tries correct fallback sequence against GTK icon theme

### Modified Capabilities
<!-- No existing capabilities are being modified -->

## Impact

- `src/ui/icon.rs`: Add `#[cfg(test)] mod icon_tests;` declaration
- `src/ui/icon_tests.rs`: New test file (~80-120 lines)
- CI: May need `xvfb-run` for headless GTK testing (future work)
