//! Failed widget indicator for displaying plugin load failures.

use gtk::prelude::*;

/// A widget that indicates a plugin failed to load.
///
/// Displays a small error indicator that can be placed in any slot
/// to show that a plugin's widget couldn't be created.
pub struct FailedWidget {
    pub root: gtk::Box,
}

impl FailedWidget {
    /// Create a new failed widget indicator.
    ///
    /// # Arguments
    /// * `plugin_id` - The ID of the plugin that failed
    /// * `error_message` - Brief description of the error
    pub fn new(plugin_id: &str, error_message: &str) -> Self {
        let root = gtk::Box::builder()
            .orientation(gtk::Orientation::Horizontal)
            .spacing(8)
            .css_classes(["failed-widget"])
            .tooltip_text(format!("{}: {}", plugin_id, error_message))
            .build();

        let icon = gtk::Image::builder()
            .icon_name("dialog-error-symbolic")
            .css_classes(["failed-widget-icon"])
            .build();

        let label = gtk::Label::builder()
            .label(plugin_id)
            .css_classes(["failed-widget-label", "dim-label"])
            .ellipsize(gtk::pango::EllipsizeMode::End)
            .build();

        root.append(&icon);
        root.append(&label);

        Self { root }
    }

    /// Get the root widget.
    pub fn widget(&self) -> &gtk::Box {
        &self.root
    }
}
