//! Button widget

use crate::types::ActionCallback;
use waft_ipc::widget::Action;
use gtk::prelude::*;

/// GTK4 button widget with optional label and icon, and click action.
pub struct ButtonWidget {
    button: gtk::Button,
}

impl ButtonWidget {
    pub fn new(
        callback: &ActionCallback,
        label: &Option<String>,
        icon: &Option<String>,
        on_click: &Action,
        widget_id: &str,
    ) -> Self {
        let button = gtk::Button::new();

        if let Some(label_text) = label {
            button.set_label(label_text);
        }

        if let Some(icon_name) = icon {
            let image = gtk::Image::from_icon_name(icon_name);
            button.set_child(Some(&image));
        }

        let widget_id = widget_id.to_string();
        let on_click = on_click.clone();
        let callback = callback.clone();

        button.connect_clicked(move |_button| {
            callback(widget_id.clone(), on_click.clone());
        });

        Self { button }
    }
}

impl crate::widget_base::WidgetBase for ButtonWidget {
    fn widget(&self) -> gtk::Widget {
        self.button.clone().upcast()
    }
}
