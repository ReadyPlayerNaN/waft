## 1. Setup and Module Structure

- [x] 1.1 Create `src/features/keyboard_layout/` directory
- [x] 1.2 Create `src/features/keyboard_layout/mod.rs` with module structure and plugin stub
- [x] 1.3 Create `src/features/keyboard_layout/dbus.rs` file for D-Bus client
- [x] 1.4 Create `src/features/keyboard_layout/widget.rs` file for GTK widget
- [x] 1.5 Add keyboard_layout module to `src/features/mod.rs`
- [x] 1.6 Add KeyboardLayoutPlugin to plugin registry initialization in `src/app.rs`

## 2. D-Bus Client Implementation

- [x] 2.1 Define `KeyboardLayoutClient` trait with methods: get_current_layout, get_available_layouts, set_layout, cycle_layout, subscribe_layout_changes
- [x] 2.2 Implement `LocaledKeyboardLayoutClient` struct with D-Bus connection field
- [x] 2.3 Implement `new()` constructor that connects to org.freedesktop.locale1
- [x] 2.4 Implement `get_current_layout()` to read X11Layout property and parse current layout
- [x] 2.5 Implement `get_available_layouts()` to parse comma-separated X11Layout property into Vec
- [x] 2.6 Implement XKB layout string parser (handle "us,de,fr", single layouts, variants, empty strings)
- [x] 2.7 Implement `set_layout()` to call SetX11Keyboard method with layout abbreviation
- [x] 2.8 Implement `cycle_layout()` that queries current, finds next in list, calls set_layout with wrap-around
- [x] 2.9 Implement `subscribe_layout_changes()` to subscribe to PropertiesChanged signals for X11Layout
- [x] 2.10 Add graceful error handling for D-Bus connection failures, method call failures, property read failures
- [x] 2.11 Add logging (info/warn/error) for D-Bus operations and failures

## 3. D-Bus Client Unit Tests

- [x] 3.1 Implement `MockKeyboardLayoutClient` struct for testing
- [x] 3.2 Add unit test for XKB layout string parsing: simple case "us" → "US"
- [x] 3.3 Add unit test for multi-layout parsing: "us,de,fr" → ["US", "DE", "FR"]
- [x] 3.4 Add unit test for layout with variant: "us(dvorak)" → "US"
- [x] 3.5 Add unit test for empty layout string handling
- [x] 3.6 Add unit test for single layout in available_layouts
- [x] 3.7 Add unit test for cycle_layout with 2 layouts (wrap-around)
- [x] 3.8 Add unit test for cycle_layout with 3+ layouts
- [x] 3.9 Add unit test for cycle_layout on last layout (wraps to first)
- [x] 3.10 Add unit test for cycle_layout with single layout (no-op)

## 4. Widget Implementation

- [x] 4.1 Define `KeyboardLayoutWidget` struct with fields: root (gtk::Button), label (gtk::Label), client (Arc<Mutex<Option<dyn KeyboardLayoutClient>>>)
- [x] 4.2 Implement `new()` constructor that creates GTK button with label
- [x] 4.3 Add CSS classes to button: `keyboard-layout-button` and label: `keyboard-layout-label`
- [x] 4.4 Implement initial layout query in constructor (async, spawn_on_tokio)
- [x] 4.5 Update button label to display current layout abbreviation in uppercase
- [x] 4.6 Implement button click handler that calls client.cycle_layout()
- [x] 4.7 Use glib::spawn_future_local + runtime::spawn_on_tokio for async D-Bus call in click handler
- [x] 4.8 Update button label after successful layout cycle
- [x] 4.9 Subscribe to PropertiesChanged signals during widget initialization
- [x] 4.10 Update button label when external layout change signal is received
- [x] 4.11 Handle D-Bus client unavailability: show fallback label "??" if client is None
- [x] 4.12 Add accessible name "Keyboard Layout" to button
- [x] 4.13 Add accessible description with current layout (e.g., "Current layout: US")
- [x] 4.14 Make button keyboard navigable (focusable, Enter/Space trigger click)

## 5. Widget Error Handling

- [x] 5.1 Implement error dialog helper function (similar to systemd_actions pattern)
- [x] 5.2 Show error dialog on cycle_layout failure with user-friendly message
- [x] 5.3 Revert button label to previous layout on cycle failure
- [x] 5.4 Handle empty layout list: show "N/A" label and disable button
- [x] 5.5 Log all widget errors (layout query failures, cycle failures, signal subscription failures)
- [x] 5.6 Handle PolicyKit authorization errors with specific error message
- [x] 5.7 Handle D-Bus connection errors with specific error message

## 6. Plugin Implementation

- [x] 6.1 Implement `KeyboardLayoutPlugin` struct with fields: dbus_client, dbus_handle
- [x] 6.2 Implement `new()` constructor
- [x] 6.3 Implement `Plugin::id()` returning PluginId::from_static("plugin::keyboard-layout")
- [x] 6.4 Implement `Plugin::init()` async method that connects to system D-Bus
- [x] 6.5 Initialize LocaledKeyboardLayoutClient in init() with graceful failure handling
- [x] 6.6 Implement `Plugin::create_elements()` async method
- [x] 6.7 Check D-Bus client availability in create_elements, skip widget if unavailable
- [x] 6.8 Create KeyboardLayoutWidget instance in create_elements
- [x] 6.9 Register widget with WidgetRegistrar: id "keyboard-layout:indicator", slot Header, weight 95
- [x] 6.10 Add module-level documentation explaining plugin purpose and D-Bus requirements

## 7. Integration and Manual Testing

- [x] 7.1 Build and run the application with keyboard layout plugin enabled
- [ ] 7.2 Verify widget appears in header at correct position (before systemd_actions)
- [ ] 7.3 Test button click cycles through configured layouts
- [ ] 7.4 Test layout change wraps around from last to first layout
- [ ] 7.5 Test external layout change via `localectl set-x11-keymap` updates widget label
- [ ] 7.6 Test widget shows fallback "??" when systemd-localed is unavailable
- [ ] 7.7 Test error dialog appears on PolicyKit authorization failure
- [ ] 7.8 Test keyboard navigation (Tab to focus, Enter/Space to cycle)
- [ ] 7.9 Test with single configured layout (verify no-op cycling)
- [ ] 7.10 Test with multiple layouts (2, 3, 4+)

## 8. Documentation

- [x] 8.1 Create `src/features/keyboard_layout/README.md`
- [x] 8.2 Add README section: Overview and features
- [x] 8.3 Add README section: D-Bus dependencies (org.freedesktop.locale1, systemd-localed)
- [x] 8.4 Add README section: How to check if systemd-localed is available
- [x] 8.5 Add README section: How to configure keyboard layouts via localectl
- [x] 8.6 Add README section: Compositor compatibility notes (Wayland compositors)
- [x] 8.7 Add README section: Troubleshooting (widget not appearing, layouts not switching)
- [x] 8.8 Add README section: Testing instructions (unit tests, integration tests)
- [x] 8.9 Add README section: PolicyKit authorization requirements
- [x] 8.10 Add code comments documenting async/await and runtime bridging patterns

## 9. Widget Tests

- [ ] 9.1 Add widget test: button has correct CSS classes
- [ ] 9.2 Add widget test: button displays current layout on initialization
- [ ] 9.3 Add widget test: button label updates on click (using mock client)
- [ ] 9.4 Add widget test: fallback label shown when client is None
- [ ] 9.5 Add widget test: accessible name is set correctly
- [ ] 9.6 Add widget test: button is focusable

## 10. Integration Tests

- [ ] 10.1 Add integration test: D-Bus client connects to localed successfully
- [ ] 10.2 Add integration test: get_current_layout returns valid layout
- [ ] 10.3 Add integration test: get_available_layouts returns non-empty list
- [ ] 10.4 Add integration test: set_layout changes layout (if D-Bus available)
- [ ] 10.5 Add integration test: PropertiesChanged signal triggers callback
- [ ] 10.6 Add integration test: graceful handling when localed is unavailable

## 11. Polish and Refinement

- [x] 11.1 Add tooltip with full layout name for accessibility (if decided during implementation)
- [x] 11.2 Implement layout list caching with signal-based refresh (if decided during implementation)
- [x] 11.3 Review all error messages for clarity and user-friendliness
- [x] 11.4 Review all log messages for appropriate log levels (info/warn/error)
- [x] 11.5 Verify async-runtime-bridge patterns are followed (no locks across await, proper spawn usage)
- [x] 11.6 Run clippy and fix any warnings
- [x] 11.7 Run rustfmt on all keyboard_layout module files
- [x] 11.8 Update main project documentation if needed (mention keyboard layout plugin)
