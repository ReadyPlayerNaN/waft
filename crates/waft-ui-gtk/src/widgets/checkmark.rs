//! Checkmark widget renderer

use gtk::prelude::*;

/// Render a Checkmark widget
///
/// Maps to gtk::Image with "object-select-symbolic" icon.
/// Applies "checkmark" CSS class and sets visibility.
pub fn render_checkmark(visible: bool) -> gtk::Widget {
    let image = gtk::Image::from_icon_name("object-select-symbolic");
    image.add_css_class("checkmark");
    image.set_visible(visible);
    image.upcast()
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
    fn test_render_checkmark_visible() {
        init_gtk();
        let widget = render_checkmark(true);

        assert!(widget.is::<gtk::Image>());
        let image: gtk::Image = widget.downcast().unwrap();
        assert!(image.is_visible());
        assert!(image.has_css_class("checkmark"));
    }

    #[test]
    #[ignore = "Requires GTK main thread - run with --test-threads=1"]
    fn test_render_checkmark_hidden() {
        init_gtk();
        let widget = render_checkmark(false);

        let image: gtk::Image = widget.downcast().unwrap();
        assert!(!image.is_visible());
        assert!(image.has_css_class("checkmark"));
    }
}
