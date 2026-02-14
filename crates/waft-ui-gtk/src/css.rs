// CSS utilities for applying GTK CSS classes to widgets

use gtk::prelude::*;

/// Apply multiple CSS classes to a widget
///
/// # Example
/// ```no_run
/// use waft_ui_gtk::css::apply_css_classes;
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
/// use waft_ui_gtk::css::add_class;
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
/// use waft_ui_gtk::css::remove_class;
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
/// use waft_ui_gtk::css::toggle_class;
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

/// Builder for efficiently managing state-based CSS classes.
///
/// Allows batching multiple conditional class updates into a single operation.
/// This is particularly useful for widgets with multiple state flags (active, busy,
/// expanded, etc.) that need to be updated atomically.
///
/// # Example
/// ```no_run
/// use waft_ui_gtk::css::CssStateBuilder;
/// gtk::init().unwrap();
/// let button = gtk::Button::new();
///
/// CssStateBuilder::new(&button)
///     .base("feature-toggle")
///     .state("active", true)
///     .state("busy", false)
///     .state("expanded", true)
///     .apply();
/// ```
pub struct CssStateBuilder<'a, W: IsA<gtk::Widget>> {
    widget: &'a W,
    base_class: Option<&'a str>,
    states: Vec<(&'a str, bool)>,
}

impl<'a, W: IsA<gtk::Widget>> CssStateBuilder<'a, W> {
    /// Create a new CssStateBuilder for a widget.
    pub fn new(widget: &'a W) -> Self {
        Self {
            widget,
            base_class: None,
            states: Vec::new(),
        }
    }

    /// Set the base CSS class that should always be present.
    ///
    /// This class will be ensured to exist after applying state changes.
    pub fn base(mut self, class: &'a str) -> Self {
        self.base_class = Some(class);
        self
    }

    /// Add a conditional state class.
    ///
    /// The class will be added if `condition` is true, removed if false.
    pub fn state(mut self, class: &'a str, condition: bool) -> Self {
        self.states.push((class, condition));
        self
    }

    /// Apply all CSS class changes to the widget.
    ///
    /// This removes all state classes first, then adds back only those with
    /// true conditions. This ensures clean state transitions without leftover classes.
    pub fn apply(self) {
        // Remove all state classes first
        for (class, _) in &self.states {
            self.widget.remove_css_class(class);
        }

        // Ensure base class exists
        if let Some(base) = self.base_class
            && !self.widget.has_css_class(base) {
                self.widget.add_css_class(base);
            }

        // Add back only active state classes
        for (class, condition) in self.states {
            if condition {
                self.widget.add_css_class(class);
            }
        }
    }
}

/// Apply state-based CSS classes in a single operation.
///
/// This is a convenience function that removes all specified state classes first,
/// ensures the base class exists, then adds back only the active states.
///
/// # Arguments
/// * `widget` - The widget to update
/// * `base_class` - Optional base class that should always be present
/// * `state_classes` - Slice of (class_name, condition) tuples
///
/// # Example
/// ```no_run
/// use waft_ui_gtk::css::apply_state_classes;
/// gtk::init().unwrap();
/// let button = gtk::Button::new();
///
/// apply_state_classes(
///     &button,
///     Some("feature-toggle"),
///     &[
///         ("active", true),
///         ("busy", false),
///         ("expanded", true),
///     ],
/// );
/// ```
pub fn apply_state_classes(
    widget: &impl IsA<gtk::Widget>,
    base_class: Option<&str>,
    state_classes: &[(&str, bool)],
) {
    // Remove all state classes first
    for (class, _) in state_classes {
        widget.remove_css_class(class);
    }

    // Ensure base class exists
    if let Some(base) = base_class
        && !widget.has_css_class(base) {
            widget.add_css_class(base);
        }

    // Add back only active state classes
    for (class, condition) in state_classes {
        if *condition {
            widget.add_css_class(class);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_utils::init_gtk_for_tests;

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
        init_gtk_for_tests();
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
        init_gtk_for_tests();
        let label = gtk::Label::new(Some("Test"));

        apply_css_classes(&label, &[]);

        // Should not panic, just do nothing
        assert!(!label.has_css_class("anything"));
    }

    #[test]
    #[ignore = "Requires GTK main thread - run with: cargo test -- --ignored --test-threads=1"]
    fn test_add_class() {
        init_gtk_for_tests();
        let label = gtk::Label::new(Some("Test"));

        add_class(&label, "bold");

        assert!(label.has_css_class("bold"));
    }

    #[test]
    #[ignore = "Requires GTK main thread - run with: cargo test -- --ignored --test-threads=1"]
    fn test_remove_class() {
        init_gtk_for_tests();
        let label = gtk::Label::new(Some("Test"));

        add_class(&label, "bold");
        assert!(label.has_css_class("bold"));

        remove_class(&label, "bold");
        assert!(!label.has_css_class("bold"));
    }

    #[test]
    #[ignore = "Requires GTK main thread - run with: cargo test -- --ignored --test-threads=1"]
    fn test_remove_class_nonexistent() {
        init_gtk_for_tests();
        let label = gtk::Label::new(Some("Test"));

        // Should not panic when removing a class that doesn't exist
        remove_class(&label, "nonexistent");
        assert!(!label.has_css_class("nonexistent"));
    }

    #[test]
    #[ignore = "Requires GTK main thread - run with: cargo test -- --ignored --test-threads=1"]
    fn test_toggle_class_true() {
        init_gtk_for_tests();
        let label = gtk::Label::new(Some("Test"));

        toggle_class(&label, "active", true);

        assert!(label.has_css_class("active"));
    }

    #[test]
    #[ignore = "Requires GTK main thread - run with: cargo test -- --ignored --test-threads=1"]
    fn test_toggle_class_false() {
        init_gtk_for_tests();
        let label = gtk::Label::new(Some("Test"));

        add_class(&label, "active");
        assert!(label.has_css_class("active"));

        toggle_class(&label, "active", false);
        assert!(!label.has_css_class("active"));
    }

    #[test]
    #[ignore = "Requires GTK main thread - run with: cargo test -- --ignored --test-threads=1"]
    fn test_toggle_class_multiple_toggles() {
        init_gtk_for_tests();
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
        init_gtk_for_tests();

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

    #[test]
    #[ignore = "Requires GTK main thread - run with: cargo test -- --ignored --test-threads=1"]
    fn test_css_state_builder_basic() {
        init_gtk_for_tests();
        let button = gtk::Button::new();

        CssStateBuilder::new(&button)
            .base("feature-toggle")
            .state("active", true)
            .state("busy", false)
            .apply();

        assert!(button.has_css_class("feature-toggle"));
        assert!(button.has_css_class("active"));
        assert!(!button.has_css_class("busy"));
    }

    #[test]
    #[ignore = "Requires GTK main thread - run with: cargo test -- --ignored --test-threads=1"]
    fn test_css_state_builder_removes_old_classes() {
        init_gtk_for_tests();
        let button = gtk::Button::new();

        // Set initial state
        button.add_css_class("active");
        button.add_css_class("busy");

        // Update state - should remove "busy"
        CssStateBuilder::new(&button)
            .base("feature-toggle")
            .state("active", true)
            .state("busy", false)
            .apply();

        assert!(button.has_css_class("active"));
        assert!(!button.has_css_class("busy"));
    }

    #[test]
    #[ignore = "Requires GTK main thread - run with: cargo test -- --ignored --test-threads=1"]
    fn test_css_state_builder_all_states_false() {
        init_gtk_for_tests();
        let button = gtk::Button::new();

        // Add some classes first
        button.add_css_class("active");
        button.add_css_class("busy");
        button.add_css_class("expanded");

        // Turn everything off
        CssStateBuilder::new(&button)
            .base("feature-toggle")
            .state("active", false)
            .state("busy", false)
            .state("expanded", false)
            .apply();

        assert!(button.has_css_class("feature-toggle"));
        assert!(!button.has_css_class("active"));
        assert!(!button.has_css_class("busy"));
        assert!(!button.has_css_class("expanded"));
    }

    #[test]
    #[ignore = "Requires GTK main thread - run with: cargo test -- --ignored --test-threads=1"]
    fn test_css_state_builder_no_base_class() {
        init_gtk_for_tests();
        let button = gtk::Button::new();

        CssStateBuilder::new(&button)
            .state("active", true)
            .state("busy", false)
            .apply();

        assert!(button.has_css_class("active"));
        assert!(!button.has_css_class("busy"));
    }

    #[test]
    #[ignore = "Requires GTK main thread - run with: cargo test -- --ignored --test-threads=1"]
    fn test_css_state_builder_preserves_base_class() {
        init_gtk_for_tests();
        let button = gtk::Button::new();

        // Base class already exists
        button.add_css_class("feature-toggle");

        CssStateBuilder::new(&button)
            .base("feature-toggle")
            .state("active", true)
            .apply();

        // Should still have base class
        assert!(button.has_css_class("feature-toggle"));
        assert!(button.has_css_class("active"));
    }

    #[test]
    #[ignore = "Requires GTK main thread - run with: cargo test -- --ignored --test-threads=1"]
    fn test_apply_state_classes_basic() {
        init_gtk_for_tests();
        let button = gtk::Button::new();

        apply_state_classes(
            &button,
            Some("feature-toggle"),
            &[("active", true), ("busy", false), ("expanded", true)],
        );

        assert!(button.has_css_class("feature-toggle"));
        assert!(button.has_css_class("active"));
        assert!(!button.has_css_class("busy"));
        assert!(button.has_css_class("expanded"));
    }

    #[test]
    #[ignore = "Requires GTK main thread - run with: cargo test -- --ignored --test-threads=1"]
    fn test_apply_state_classes_clean_transition() {
        init_gtk_for_tests();
        let button = gtk::Button::new();

        // Set initial state
        apply_state_classes(
            &button,
            Some("feature-toggle"),
            &[("active", true), ("busy", true), ("expanded", false)],
        );

        assert!(button.has_css_class("active"));
        assert!(button.has_css_class("busy"));
        assert!(!button.has_css_class("expanded"));

        // Update state - should cleanly transition
        apply_state_classes(
            &button,
            Some("feature-toggle"),
            &[("active", false), ("busy", false), ("expanded", true)],
        );

        assert!(!button.has_css_class("active"));
        assert!(!button.has_css_class("busy"));
        assert!(button.has_css_class("expanded"));
    }

    #[test]
    #[ignore = "Requires GTK main thread - run with: cargo test -- --ignored --test-threads=1"]
    fn test_apply_state_classes_no_base() {
        init_gtk_for_tests();
        let button = gtk::Button::new();

        apply_state_classes(&button, None, &[("active", true)]);

        assert!(button.has_css_class("active"));
    }

    #[test]
    #[ignore = "Requires GTK main thread - run with: cargo test -- --ignored --test-threads=1"]
    fn test_apply_state_classes_empty_states() {
        init_gtk_for_tests();
        let button = gtk::Button::new();

        // Should not panic with empty states
        apply_state_classes(&button, Some("base"), &[]);

        assert!(button.has_css_class("base"));
    }
}
