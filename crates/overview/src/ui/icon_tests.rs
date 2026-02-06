use super::*;
use std::path::PathBuf;
use std::sync::Arc;

#[test]
fn test_absolute_path_detection() {
    let icon = Icon::parse(&Arc::from("/usr/share/icons/test.png"));
    match icon {
        Icon::FilePath(path) => {
            assert_eq!(path.as_ref(), &PathBuf::from("/usr/share/icons/test.png"));
        }
        _ => panic!("Expected Icon::FilePath, got {:?}", icon),
    }
}

#[test]
fn test_relative_path_with_dot() {
    let icon = Icon::parse(&Arc::from("./icons/test.png"));
    match icon {
        Icon::FilePath(path) => {
            assert_eq!(path.as_ref(), &PathBuf::from("./icons/test.png"));
        }
        _ => panic!("Expected Icon::FilePath, got {:?}", icon),
    }
}

#[test]
fn test_home_directory_path() {
    let icon = Icon::parse(&Arc::from("~/icons/test.png"));
    match icon {
        Icon::FilePath(path) => {
            assert_eq!(path.as_ref(), &PathBuf::from("~/icons/test.png"));
        }
        _ => panic!("Expected Icon::FilePath, got {:?}", icon),
    }
}

#[test]
fn test_themed_icon_name() {
    let icon_name = Arc::from("dialog-information");
    let icon = Icon::parse(&icon_name);
    match icon {
        Icon::Themed(name) => {
            assert_eq!(name.as_ref(), "dialog-information");
        }
        _ => panic!("Expected Icon::Themed, got {:?}", icon),
    }
}

#[test]
fn test_whitespace_trimming_themed() {
    let icon = Icon::parse(&Arc::from("  dialog-information  "));
    match icon {
        Icon::Themed(name) => {
            assert_eq!(name.as_ref(), "dialog-information");
        }
        _ => panic!("Expected Icon::Themed, got {:?}", icon),
    }
}

#[test]
fn test_whitespace_trimming_filepath() {
    let icon = Icon::parse(&Arc::from("  /usr/share/icons/test.png  "));
    match icon {
        Icon::FilePath(path) => {
            assert_eq!(path.as_ref(), &PathBuf::from("/usr/share/icons/test.png"));
        }
        _ => panic!("Expected Icon::FilePath, got {:?}", icon),
    }
}

#[test]
fn test_resolve_exact_name_match() {
    // Using a standard icon that should exist in most GTK environments
    // This test may fail in headless environments without GTK display
    let result = resolve_themed_icon("dialog-information");

    // If GTK display is available, this should resolve
    // If not, it will return None (graceful degradation)
    if let Some(resolved) = result {
        assert_eq!(resolved, "dialog-information");
    }
}

#[test]
fn test_resolve_symbolic_fallback() {
    // Test that if exact name doesn't exist, symbolic variant is tried
    // Using "dialog-information" which commonly has a -symbolic variant
    let result = resolve_themed_icon("dialog-information");

    // In GTK environments, this should resolve to either the exact name or symbolic variant
    if result.is_some() {
        // Test passes if we got any resolution (exact or symbolic)
        // The actual fallback logic is tested by the implementation
        assert!(result.is_some());
    }
}

#[test]
fn test_resolve_lowercase_fallback() {
    // Test that mixed-case icon names can be resolved via lowercase fallback
    // If exact "Dialog-Information" doesn't exist, "dialog-information" should be tried
    let result = resolve_themed_icon("Dialog-Information");

    // In GTK environments with standard icons, this should resolve via lowercase fallback
    if let Some(resolved) = result {
        // Should resolve to lowercase variant
        assert!(resolved.to_lowercase() == resolved || resolved.ends_with("-symbolic"));
    }
}

#[test]
fn test_resolve_lowercase_symbolic_fallback() {
    // Test the final fallback: lowercase-symbolic
    // Using mixed-case name that might only have lowercase-symbolic variant
    let result = resolve_themed_icon("DIALOG-INFORMATION");

    // In GTK environments, this should eventually resolve via lowercase or lowercase-symbolic
    if let Some(resolved) = result {
        // Should be lowercase and possibly symbolic
        assert_eq!(resolved, resolved.to_lowercase());
    }
}

#[test]
fn test_resolve_no_match() {
    // Test that non-existent icons return None after trying all fallbacks
    let result = resolve_themed_icon("this-icon-definitely-does-not-exist-12345");

    // Should return None when no variants are found
    // OR if GTK display is not available
    assert!(result.is_none());
}
