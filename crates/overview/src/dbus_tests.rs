// Integration test helper for DBus functionality.
//
// Note: Full integration tests with mock DBus servers require more setup
// and are omitted from this initial implementation. The owned_value_to_string
// function is tested indirectly through the DBus property operations.
//
// Future work: Add integration tests using zbus::dbus_interface macro to create
// mock services for testing property get/set and signal listening.
