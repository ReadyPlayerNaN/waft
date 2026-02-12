//! Pure GTK4 InfoCard widget.
//!
//! A card with icon, title, optional description, and optional click action.
//! Layout: `[Icon 32x32] [Title (bold) / Description (dim)]`
//! When on_click is Some, the card is wrapped in a flat button.

use gtk::prelude::*;

use crate::types::ActionCallback;
use crate::widgets::icon::IconWidget;
use waft_ipc::widget::Action;
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
        let (root, icon_widget, title_label, description_label) =
            Self::build_content(icon, title, description, false);

        Self {
            root,
            icon_widget,
            title_label,
            description_label,
        }
    }

    /// Create a new clickable info card widget.
    pub fn new_clickable(
        icon: &str,
        title: &str,
        description: Option<&str>,
        callback: &ActionCallback,
        on_click: &Action,
        widget_id: &str,
    ) -> Self {
        let (root, icon_widget, title_label, description_label) =
            Self::build_content(icon, title, description, true);

        let cb = callback.clone();
        let wid = widget_id.to_string();
        let action = on_click.clone();
        let button: gtk::Button = root.clone().downcast().unwrap();
        button.connect_clicked(move |_| {
            cb(wid.clone(), action.clone());
        });

        Self {
            root,
            icon_widget,
            title_label,
            description_label,
        }
    }

    fn build_content(
        icon: &str,
        title: &str,
        description: Option<&str>,
        clickable: bool,
    ) -> (gtk::Widget, IconWidget, gtk::Label, gtk::Label) {
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

        let root: gtk::Widget = if clickable {
            let button = gtk::Button::builder()
                .css_classes(["flat", "info-card"])
                .child(&content_box)
                .build();
            button.upcast()
        } else {
            content_box.upcast()
        };

        (root, icon_widget, title_label, description_label)
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
