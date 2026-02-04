//! Tests for brightness backend parsing.

use super::*;

#[test]
fn test_humanize_backlight_name_intel() {
    assert_eq!(
        humanize_backlight_name("intel_backlight"),
        "Built-in Display"
    );
}

#[test]
fn test_humanize_backlight_name_amd() {
    assert_eq!(humanize_backlight_name("amdgpu_bl0"), "Built-in Display");
}

#[test]
fn test_humanize_backlight_name_unknown() {
    assert_eq!(humanize_backlight_name("some_device"), "Some Device");
}
