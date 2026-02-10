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
    fn test_render_spinner_spinning() {
        init_gtk();
        let widget = render_spinner(true);

        assert!(widget.is::<gtk::Spinner>());
        let spinner: gtk::Spinner = widget.downcast().unwrap();
        assert!(spinner.is_spinning());
    }

    #[test]
    #[ignore = "Requires GTK main thread - run with --test-threads=1"]
    fn test_render_spinner_not_spinning() {
        init_gtk();
        let widget = render_spinner(false);

        let spinner: gtk::Spinner = widget.downcast().unwrap();
        assert!(!spinner.is_spinning());
    }
}
