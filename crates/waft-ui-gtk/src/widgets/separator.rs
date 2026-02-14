//! Separator widget - visual separator line

use gtk::prelude::*;

/// GTK4 separator widget for visual grouping.
pub struct SeparatorWidget {
    separator: gtk::Separator,
}

impl Default for SeparatorWidget {
    fn default() -> Self {
        Self::new()
    }
}

impl SeparatorWidget {
    pub fn new() -> Self {
        let separator = gtk::Separator::new(gtk::Orientation::Horizontal);
        Self { separator }
    }
}

impl crate::widget_base::WidgetBase for SeparatorWidget {
    fn widget(&self) -> gtk::Widget {
        self.separator.clone().upcast()
    }
}
