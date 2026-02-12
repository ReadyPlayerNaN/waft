//! ListRow widget -- a horizontal row of children with CSS classes.

use crate::css::apply_css_classes;
use crate::widget_base::Children;
use gtk::prelude::*;

/// GTK4 list row widget -- a horizontal box with children and CSS classes.
pub struct ListRowWidget {
    row: gtk::Box,
}

impl ListRowWidget {
    pub fn new(children: Children, css_classes: &[String]) -> Self {
        let row = gtk::Box::builder()
            .orientation(gtk::Orientation::Horizontal)
            .spacing(8)
            .build();

        apply_css_classes(&row, css_classes);

        for child in children.iter_widgets() {
            row.append(&child);
        }

        Self { row }
    }
}

impl crate::widget_base::WidgetBase for ListRowWidget {
    fn widget(&self) -> gtk::Widget {
        self.row.clone().upcast()
    }
}
