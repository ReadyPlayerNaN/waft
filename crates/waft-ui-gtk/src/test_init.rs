//! Testing utilities for waft-ui-gtk tests.
//!
//! This module provides common test infrastructure to reduce boilerplate
//! across widget test files.

use std::sync::Once;

/// Initialize GTK for tests.
///
/// This function ensures GTK is initialized exactly once across all tests
/// in the test suite. It's safe to call multiple times - subsequent calls
/// are no-ops.
///
/// # Example
///
/// ```rust,no_run
/// use waft_ui_gtk::test_init::init_gtk_for_tests;
///
/// #[test]
/// fn test_widget() {
///     init_gtk_for_tests();
///     // ... test code using GTK widgets
/// }
/// ```
pub fn init_gtk_for_tests() -> bool {
    static GTK_INIT: Once = Once::new();
    static GTK_READY: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
    GTK_INIT.call_once(|| {
        if gtk::init().is_ok() {
            GTK_READY.store(true, std::sync::atomic::Ordering::Relaxed);
        }
    });
    GTK_READY.load(std::sync::atomic::Ordering::Relaxed)
}
