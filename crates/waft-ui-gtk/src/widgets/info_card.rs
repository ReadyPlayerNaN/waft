//! Pure GTK4 InfoCard widget.
//!
//! A card with icon, title, and optional description.
//! Layout: `[Icon 32x32] [Title (bold) / Description (dim)]`

use gtk::prelude::*;

use crate::icons::IconWidget;

/// Pure GTK4 info card widget.
#[derive(Clone)]
pub struct InfoCardWidget {
    root: gtk::Widget,
    icon_widget: IconWidget,
    title_label: gtk::Label,
    description_label: gtk::Label,
}

impl InfoCardWidget {
    /// Create a new info card widget.
    pub fn new(icon: &str, title: &str, description: Option<&str>) -> Self {
        let content_box = gtk::Box::builder()
            .orientation(gtk::Orientation::Horizontal)
            .spacing(8)
            .build();

        let icon_widget = IconWidget::from_name(icon, 32);
        icon_widget.widget().set_height_request(32);

        let labels_box = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .spacing(0)
            .valign(gtk::Align::Center)
            .build();

        let title_label = gtk::Label::builder()
            .label(title)
            .css_classes(["title-3"])
            .xalign(0.0)
            .build();

        let description_label = gtk::Label::builder()
            .label(description.unwrap_or(""))
            .css_classes(["dim-label"])
            .xalign(0.0)
            .visible(description.is_some())
            .build();

        labels_box.append(&title_label);
        labels_box.append(&description_label);

        content_box.append(icon_widget.widget());
        content_box.append(&labels_box);

        Self {
            root: content_box.upcast(),
            icon_widget,
            title_label,
            description_label,
        }
    }

    /// Update the icon.
    pub fn set_icon(&self, icon: &str) {
        self.icon_widget.set_icon(icon);
    }

    /// Update the title text.
    pub fn set_title(&self, title: &str) {
        self.title_label.set_label(title);
    }

    /// Update the description text and visibility.
    pub fn set_description(&self, description: Option<&str>) {
        match description {
            Some(text) => {
                self.description_label.set_label(text);
                self.description_label.set_visible(true);
            }
            None => {
                self.description_label.set_label("");
                self.description_label.set_visible(false);
            }
        }
    }

    /// Get a reference to the root widget.
    pub fn widget(&self) -> gtk::Widget {
        self.root.clone()
    }
}

impl crate::widget_base::WidgetBase for InfoCardWidget {
    fn widget(&self) -> gtk::Widget {
        self.widget()
    }
}
