//! Row widget - horizontal layout container

use crate::css::apply_css_classes;
use crate::widget_base::Children;
use gtk::prelude::*;

/// GTK4 horizontal layout container widget.
pub struct RowWidget {
    container: gtk::Box,
}

impl RowWidget {
    pub fn new(spacing: u32, css_classes: &[String], children: Children) -> Self {
        let container = gtk::Box::new(gtk::Orientation::Horizontal, spacing as i32);
        apply_css_classes(&container, css_classes);

        for child in children.iter_widgets() {
            container.append(&child);
        }

        Self { container }
    }
}

impl crate::widget_base::WidgetBase for RowWidget {
    fn widget(&self) -> gtk::Widget {
        self.container.clone().upcast()
    }
}
