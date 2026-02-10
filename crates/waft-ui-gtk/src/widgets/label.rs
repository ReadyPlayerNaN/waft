//! Label widget renderer

use crate::css::apply_css_classes;
use gtk::prelude::*;

/// Render a Label widget
///
/// Maps to gtk::Label with text and CSS classes.
pub fn render_label(text: &str, css_classes: &[String]) -> gtk::Widget {
    let label = gtk::Label::new(Some(text));
    apply_css_classes(&label, css_classes);
    label.upcast()
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

    #[test]
    #[ignore = "Requires GTK main thread - run with --test-threads=1"]
    fn test_render_label_basic() {
        init_gtk();
        let widget = render_label("Hello World", &[]);

        assert!(widget.is::<gtk::Label>());
        let label: gtk::Label = widget.downcast().unwrap();
        assert_eq!(label.text(), "Hello World");
    }

    #[test]
    #[ignore = "Requires GTK main thread - run with --test-threads=1"]
    fn test_render_label_with_css_classes() {
        init_gtk();
        let classes = vec!["bold".to_string(), "accent".to_string()];
        let widget = render_label("Styled Label", &classes);

        let label: gtk::Label = widget.downcast().unwrap();
        assert_eq!(label.text(), "Styled Label");
        assert!(label.has_css_class("bold"));
        assert!(label.has_css_class("accent"));
    }

    #[test]
    #[ignore = "Requires GTK main thread - run with --test-threads=1"]
    fn test_render_label_empty_text() {
        init_gtk();
        let widget = render_label("", &[]);

        let label: gtk::Label = widget.downcast().unwrap();
        assert_eq!(label.text(), "");
    }
}
