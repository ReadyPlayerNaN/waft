## 1. Add shared helpers to src/dbus.rs

- [x] 1.1 Add `owned_value_to_bool` function (move from bluetooth)
- [x] 1.2 Add `owned_value_to_u32` function
- [x] 1.3 Add `owned_value_to_i64` function
- [x] 1.4 Add `owned_value_to_f64` function
- [x] 1.5 Add `DbusHandle::get_typed_property<T>` generic method with default fallback
- [x] 1.6 Add `DbusHandle::get_all_properties` method wrapping GetAll
- [x] 1.7 Add `DbusHandle::listen_properties_changed` helper method

## 2. Add tests for src/dbus.rs core functionality

- [x] 2.1 Create `src/dbus_tests.rs` test file
- [x] 2.2 Add `#[cfg(test)] mod dbus_tests;` to `src/dbus.rs`
- [x] 2.3 Add unit tests for value extractors (bool, u32, i64, f64, string)
- [x] 2.4 Add test helper for creating mock DBus server using zbus test utilities
- [x] 2.5 Add integration tests for `DbusHandle::connect()` and `connect_system()`
- [x] 2.6 Add integration tests for `get_property()` and `set_property()`
- [x] 2.7 Add integration tests for `listen_signals()` with match rules
- [x] 2.8 Add integration tests for `listen_for_values()` callback
- [x] 2.9 Add tests for `get_typed_property<T>` with various types
- [x] 2.10 Add tests for `get_all_properties()` GetAll wrapper
- [x] 2.11 Add tests for `listen_properties_changed()` signal helper
- [ ] 2.12 Run tests with `cargo test dbus_tests` and verify all pass

## 3. Update comments in src/dbus.rs

- [x] 3.1 Remove redundant explanations of basic DBus concepts
- [x] 3.2 Remove verbose parameter descriptions repeated from signatures
- [x] 3.3 Convert multi-line comments to concise doc comments (`///`)
- [x] 3.4 Update outdated references and remove completed TODOs
- [x] 3.5 Keep workaround explanations (local filtering, MessageStream behavior)
- [x] 3.6 Verify all public functions have concise doc comments

## 4. Refactor darkman module

- [x] 4.1 Update `src/features/darkman/dbus.rs` to use shared helpers if applicable
- [x] 4.2 Create `src/features/darkman/dbus_tests.rs`
- [x] 4.3 Add tests for `get_state()` function
- [x] 4.4 Add tests for `set_state()` function
- [x] 4.5 Update comments following guidelines
- [x] 4.6 Run tests and verify darkman functionality unchanged

## 5. Refactor battery module

- [x] 5.1 Update `src/features/battery/dbus.rs` to use `get_all_properties()` helper
- [x] 5.2 Update to use `listen_properties_changed()` if beneficial
- [x] 5.3 Create `src/features/battery/dbus_tests.rs`
- [x] 5.4 Add tests for `get_battery_info()` function
- [x] 5.5 Add tests for `listen_battery_changes()` signal listener
- [x] 5.6 Update comments following guidelines
- [x] 5.7 Run tests and verify battery functionality unchanged

## 6. Refactor bluetooth module

- [x] 6.1 Remove duplicated `owned_value_to_bool` and `owned_value_to_string` (use shared versions)
- [x] 6.2 Update property parsing to use shared value extractors
- [x] 6.3 Consider using `get_all_properties()` helper for ObjectManager parsing
- [x] 6.4 Create `src/features/bluetooth/dbus_tests.rs`
- [x] 6.5 Add tests for `find_all_adapters()` function
- [x] 6.6 Add tests for `get_powered()` and `set_powered()` functions
- [x] 6.7 Add tests for `find_paired_devices()` function
- [x] 6.8 Update comments following guidelines
- [x] 6.9 Run tests and verify bluetooth functionality unchanged

## 7. Refactor networkmanager module

- [x] 7.1 Replace raw `Connection::system()` calls with `DbusHandle::connect_system()`
- [x] 7.2 Update all functions to accept `&DbusHandle` parameter
- [x] 7.3 Update `get_device_property()` to use `get_typed_property<T>()` helper
- [x] 7.4 Create `src/features/networkmanager/dbus_tests.rs`
- [x] 7.5 Add tests for `check_availability()` function
- [x] 7.6 Add tests for `get_all_devices()` function
- [x] 7.7 Add tests for `get_device_property()` typed getter
- [x] 7.8 Update comments following guidelines
- [x] 7.9 Run tests and verify networkmanager functionality unchanged

## 8. Add tests for audio module

- [x] 8.1 Create `src/features/audio/dbus_tests.rs`
- [x] 8.2 Add tests for `get_card_port_info()` pactl parsing
- [x] 8.3 Add tests for `get_sinks()` pactl parsing
- [x] 8.4 Add tests for `get_sources()` pactl parsing
- [x] 8.5 Add tests for `set_default_sink()` function
- [x] 8.6 Add tests for `set_sink_volume()` function
- [x] 8.7 Update comments following guidelines (limited refactoring - pactl-based)
- [x] 8.8 Run tests and verify audio functionality unchanged

## 9. Refactor agenda module

- [x] 9.1 Update `src/features/agenda/dbus.rs` to use shared helpers where applicable
- [x] 9.2 Create `src/features/agenda/dbus_tests.rs`
- [x] 9.3 Add tests for calendar event fetching
- [x] 9.4 Update comments following guidelines
- [x] 9.5 Run tests and verify agenda functionality unchanged

## 10. Integration verification

- [x] 10.1 Run full test suite with `cargo test`
- [x] 10.2 Verify all DBus tests pass
- [ ] 10.3 Run application and manually verify all DBus features work (darkman, battery, bluetooth, audio, networkmanager, agenda)
- [x] 10.4 Check test coverage for all DBus modules
- [ ] 10.5 Document any CI requirements (xvfb for GTK tests if needed)
