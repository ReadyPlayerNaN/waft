//! Spinner widget renderer

use gtk::prelude::*;

/// Render a Spinner widget
///
/// Maps to gtk::Spinner. Calls start() if spinning is true.
pub fn render_spinner(spinning: bool) -> gtk::Widget {
    let spinner = gtk::Spinner::new();
    if spinning {
        spinner.start();
    }
    spinner.upcast()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_utils::init_gtk_for_tests;

    #[test]
    #[ignore = "Requires GTK main thread - run with --test-threads=1"]
    fn test_render_spinner_spinning() {
        init_gtk_for_tests();
        let widget = render_spinner(true);

        assert!(widget.is::<gtk::Spinner>());
        let spinner: gtk::Spinner = widget.downcast().unwrap();
        assert!(spinner.is_spinning());
    }

    #[test]
    #[ignore = "Requires GTK main thread - run with --test-threads=1"]
    fn test_render_spinner_not_spinning() {
        init_gtk_for_tests();
        let widget = render_spinner(false);

        let spinner: gtk::Spinner = widget.downcast().unwrap();
        assert!(!spinner.is_spinning());
    }
}
