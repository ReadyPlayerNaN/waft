use crate::features::darkman::values::DarkmanMode;

#[test]
fn test_darkman_mode_from_str_dark() {
    assert_eq!(DarkmanMode::from_str("dark"), Some(DarkmanMode::Dark));
}

#[test]
fn test_darkman_mode_from_str_light() {
    assert_eq!(DarkmanMode::from_str("light"), Some(DarkmanMode::Light));
}

#[test]
fn test_darkman_mode_from_str_invalid() {
    assert_eq!(DarkmanMode::from_str("invalid"), None);
    assert_eq!(DarkmanMode::from_str(""), None);
    assert_eq!(DarkmanMode::from_str("Dark"), None); // Case sensitive
}

#[test]
fn test_darkman_mode_as_str_dark() {
    assert_eq!(DarkmanMode::Dark.as_str(), "dark");
}

#[test]
fn test_darkman_mode_as_str_light() {
    assert_eq!(DarkmanMode::Light.as_str(), "light");
}

#[test]
fn test_darkman_mode_roundtrip() {
    // Test that as_str -> from_str roundtrips correctly
    assert_eq!(
        DarkmanMode::from_str(DarkmanMode::Dark.as_str()),
        Some(DarkmanMode::Dark)
    );
    assert_eq!(
        DarkmanMode::from_str(DarkmanMode::Light.as_str()),
        Some(DarkmanMode::Light)
    );
}

#[test]
fn test_darkman_mode_default() {
    assert_eq!(DarkmanMode::default(), DarkmanMode::Light);
}

#[test]
fn test_darkman_mode_is_active_dark() {
    assert!(DarkmanMode::Dark.is_active());
}

#[test]
fn test_darkman_mode_is_active_light() {
    assert!(!DarkmanMode::Light.is_active());
}

// Integration tests for get_state() and set_state() require a mock DBus server.
// These are deferred similar to src/dbus_tests.rs approach.
//
// Future work: Add integration tests using zbus::dbus_interface macro to create
// a mock darkman service for testing get_state() and set_state() functions.
//
// Test scenarios to add:
// - get_state() returns Dark when DBus property is "dark"
// - get_state() returns Light when DBus property is "light"
// - get_state() returns Light (default) when property is missing
// - get_state() returns Light (default) when property is invalid
// - set_state() successfully sets property to "dark"
// - set_state() successfully sets property to "light"
