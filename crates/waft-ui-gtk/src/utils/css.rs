// CSS utilities for applying GTK CSS classes to widgets

use gtk::prelude::*;

/// Apply multiple CSS classes to a widget
///
/// # Example
/// ```no_run
/// use waft_ui_gtk::utils::css::apply_css_classes;
/// gtk::init().unwrap();
/// let label = gtk::Label::new(Some("Hello"));
/// apply_css_classes(&label, &vec!["dim-label".to_string(), "small".to_string()]);
/// ```
pub fn apply_css_classes(widget: &impl IsA<gtk::Widget>, classes: &[String]) {
    for class in classes {
        widget.add_css_class(class);
    }
}

/// Add a single CSS class to a widget
///
/// # Example
/// ```no_run
/// use waft_ui_gtk::utils::css::add_class;
/// gtk::init().unwrap();
/// let label = gtk::Label::new(Some("Hello"));
/// add_class(&label, "bold");
/// ```
pub fn add_class(widget: &impl IsA<gtk::Widget>, class: &str) {
    widget.add_css_class(class);
}

/// Remove a single CSS class from a widget
///
/// # Example
/// ```no_run
/// use waft_ui_gtk::utils::css::remove_class;
/// gtk::init().unwrap();
/// let label = gtk::Label::new(Some("Hello"));
/// remove_class(&label, "bold");
/// ```
pub fn remove_class(widget: &impl IsA<gtk::Widget>, class: &str) {
    widget.remove_css_class(class);
}

/// Conditionally add or remove a CSS class based on a condition
///
/// If `condition` is true, the class is added. If false, it is removed.
///
/// # Example
/// ```no_run
/// use waft_ui_gtk::utils::css::toggle_class;
/// gtk::init().unwrap();
/// let label = gtk::Label::new(Some("Hello"));
/// toggle_class(&label, "active", true);  // adds "active"
/// toggle_class(&label, "active", false); // removes "active"
/// ```
pub fn toggle_class(widget: &impl IsA<gtk::Widget>, class: &str, condition: bool) {
    if condition {
        widget.add_css_class(class);
    } else {
        widget.remove_css_class(class);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Helper to ensure GTK is initialized only once for all tests
    fn init_gtk() {
        use std::sync::Once;
        static INIT: Once = Once::new();
        INIT.call_once(|| {
            gtk::init().expect("Failed to initialize GTK");
        });
    }

    // NOTE: GTK widget tests require the main thread to run.
    // Rust's test harness spawns tests on worker threads, which causes GTK to panic.
    // These tests are marked #[ignore] and should be run manually with:
    //   cargo test -p waft-ui-gtk -- --ignored --test-threads=1
    //
    // This runs tests sequentially and increases (but doesn't guarantee) the chance
    // they'll run on the main thread. This is a known limitation of GTK + Rust testing.
    //
    // For CI/CD, consider using a custom test harness or integration tests with
    // explicit main thread control.

    #[test]
    #[ignore = "Requires GTK main thread - run with: cargo test -- --ignored --test-threads=1"]
    fn test_apply_css_classes() {
        init_gtk();
        let label = gtk::Label::new(Some("Test"));

        let classes = vec!["class1".to_string(), "class2".to_string(), "class3".to_string()];
        apply_css_classes(&label, &classes);

        assert!(label.has_css_class("class1"));
        assert!(label.has_css_class("class2"));
        assert!(label.has_css_class("class3"));
    }

    #[test]
    #[ignore = "Requires GTK main thread - run with: cargo test -- --ignored --test-threads=1"]
    fn test_apply_css_classes_empty() {
        init_gtk();
        let label = gtk::Label::new(Some("Test"));

        apply_css_classes(&label, &[]);

        // Should not panic, just do nothing
        assert!(!label.has_css_class("anything"));
    }

    #[test]
    #[ignore = "Requires GTK main thread - run with: cargo test -- --ignored --test-threads=1"]
    fn test_add_class() {
        init_gtk();
        let label = gtk::Label::new(Some("Test"));

        add_class(&label, "bold");

        assert!(label.has_css_class("bold"));
    }

    #[test]
    #[ignore = "Requires GTK main thread - run with: cargo test -- --ignored --test-threads=1"]
    fn test_remove_class() {
        init_gtk();
        let label = gtk::Label::new(Some("Test"));

        add_class(&label, "bold");
        assert!(label.has_css_class("bold"));

        remove_class(&label, "bold");
        assert!(!label.has_css_class("bold"));
    }

    #[test]
    #[ignore = "Requires GTK main thread - run with: cargo test -- --ignored --test-threads=1"]
    fn test_remove_class_nonexistent() {
        init_gtk();
        let label = gtk::Label::new(Some("Test"));

        // Should not panic when removing a class that doesn't exist
        remove_class(&label, "nonexistent");
        assert!(!label.has_css_class("nonexistent"));
    }

    #[test]
    #[ignore = "Requires GTK main thread - run with: cargo test -- --ignored --test-threads=1"]
    fn test_toggle_class_true() {
        init_gtk();
        let label = gtk::Label::new(Some("Test"));

        toggle_class(&label, "active", true);

        assert!(label.has_css_class("active"));
    }

    #[test]
    #[ignore = "Requires GTK main thread - run with: cargo test -- --ignored --test-threads=1"]
    fn test_toggle_class_false() {
        init_gtk();
        let label = gtk::Label::new(Some("Test"));

        add_class(&label, "active");
        assert!(label.has_css_class("active"));

        toggle_class(&label, "active", false);
        assert!(!label.has_css_class("active"));
    }

    #[test]
    #[ignore = "Requires GTK main thread - run with: cargo test -- --ignored --test-threads=1"]
    fn test_toggle_class_multiple_toggles() {
        init_gtk();
        let label = gtk::Label::new(Some("Test"));

        toggle_class(&label, "active", true);
        assert!(label.has_css_class("active"));

        toggle_class(&label, "active", false);
        assert!(!label.has_css_class("active"));

        toggle_class(&label, "active", true);
        assert!(label.has_css_class("active"));
    }

    #[test]
    #[ignore = "Requires GTK main thread - run with: cargo test -- --ignored --test-threads=1"]
    fn test_works_with_different_widget_types() {
        init_gtk();

        // Test with Label
        let label = gtk::Label::new(Some("Test"));
        add_class(&label, "test-class");
        assert!(label.has_css_class("test-class"));

        // Test with Button
        let button = gtk::Button::new();
        add_class(&button, "test-class");
        assert!(button.has_css_class("test-class"));

        // Test with Box
        let container = gtk::Box::new(gtk::Orientation::Horizontal, 0);
        add_class(&container, "test-class");
        assert!(container.has_css_class("test-class"));
    }
}
