//! IconList widget -- a list with a leading icon and child widgets.

use crate::widget_base::Children;
use crate::icons::IconWidget;
use gtk::prelude::*;

/// GTK4 icon list widget -- horizontal box with icon + vertical children.
pub struct IconListWidget {
    root: gtk::Box,
}

impl IconListWidget {
    pub fn new(icon: &str, icon_size: i32, children: Children) -> Self {
        let root = gtk::Box::builder()
            .orientation(gtk::Orientation::Horizontal)
            .spacing(8)
            .build();

        let icon_widget = IconWidget::from_name(icon, icon_size);
        icon_widget.widget().set_valign(gtk::Align::Start);
        root.append(icon_widget.widget());

        let content = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .spacing(4)
            .hexpand(true)
            .build();

        for child in children.iter_widgets() {
            content.append(&child);
        }

        root.append(&content);

        Self { root }
    }
}

impl crate::widget_base::WidgetBase for IconListWidget {
    fn widget(&self) -> gtk::Widget {
        self.root.clone().upcast()
    }
}
