//! Separator widget renderer - visual separator line

use gtk::prelude::*;

/// Render a Separator widget as a gtk::Separator
///
/// Creates a horizontal separator line for visual grouping of menu items.
///
/// # Returns
///
/// A gtk::Separator widget, upcast to gtk::Widget
pub fn render_separator() -> gtk::Widget {
    let separator = gtk::Separator::new(gtk::Orientation::Horizontal);
    separator.upcast()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_utils::init_gtk_for_tests;

    #[test]
    #[ignore = "Requires GTK main thread - run with --test-threads=1"]
    fn test_render_separator() {
        init_gtk_for_tests();

        let widget = render_separator();

        assert!(widget.is::<gtk::Separator>());
        let separator: gtk::Separator = widget.downcast().unwrap();
        assert_eq!(separator.orientation(), gtk::Orientation::Horizontal);
    }
}
