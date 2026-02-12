//! ListButton widget -- a button styled for use in lists.

use crate::css::apply_css_classes;
use crate::types::ActionCallback;
use gtk::prelude::*;
use waft_ipc::widget::Action;

/// GTK4 list button widget with optional icon, label, CSS classes, and click action.
pub struct ListButtonWidget {
    button: gtk::Button,
}

impl ListButtonWidget {
    pub fn new(
        callback: &ActionCallback,
        label: &str,
        icon: &Option<String>,
        css_classes: &[String],
        on_click: &Action,
        widget_id: &str,
    ) -> Self {
        let content = gtk::Box::builder()
            .orientation(gtk::Orientation::Horizontal)
            .spacing(8)
            .build();

        if let Some(icon_name) = icon {
            let image = gtk::Image::from_icon_name(icon_name);
            content.append(&image);
        }

        let label_widget = gtk::Label::new(Some(label));
        content.append(&label_widget);

        let button = gtk::Button::builder()
            .css_classes(["flat"])
            .child(&content)
            .build();

        apply_css_classes(&button, css_classes);

        let cb = callback.clone();
        let wid = widget_id.to_string();
        let action = on_click.clone();
        button.connect_clicked(move |_| {
            cb(wid.clone(), action.clone());
        });

        Self { button }
    }
}

impl crate::widget_base::WidgetBase for ListButtonWidget {
    fn widget(&self) -> gtk::Widget {
        self.button.clone().upcast()
    }
}
