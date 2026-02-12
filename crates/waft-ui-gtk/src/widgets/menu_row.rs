//! MenuRow widget - converts MenuRow descriptions to GTK button with layout

use crate::types::ActionCallback;
use crate::widget_base::Child;
use crate::widgets::icon::IconWidget;
use gtk::prelude::*;
use waft_ipc::widget::Action;

/// GTK4 menu row widget -- a button with horizontal layout containing icon, label,
/// optional spinner (when busy), and optional trailing widget.
pub struct MenuRowWidget {
    button: gtk::Button,
}

impl MenuRowWidget {
    pub fn new(
        callback: &ActionCallback,
        icon: &Option<String>,
        label: &str,
        trailing: Option<Child>,
        sensitive: bool,
        busy: bool,
        on_click: &Option<Action>,
        widget_id: &str,
    ) -> Self {
        let button = gtk::Button::new();
        button.set_sensitive(sensitive && !busy);

        let container = gtk::Box::new(gtk::Orientation::Horizontal, 12);

        if let Some(icon_name) = icon {
            let icon_widget = IconWidget::from_name(icon_name, 24);
            container.append(icon_widget.widget());
        }

        let main_label = gtk::Label::new(Some(label));
        main_label.set_hexpand(true);
        main_label.set_halign(gtk::Align::Start);
        main_label.set_valign(gtk::Align::Center);
        container.append(&main_label);

        if busy {
            let spinner = gtk::Spinner::new();
            spinner.set_spinning(true);
            container.append(&spinner);
        }

        if let Some(child) = trailing {
            container.append(&child.widget());
        }

        button.set_child(Some(&container));

        if let Some(action) = on_click {
            let widget_id = widget_id.to_string();
            let action = action.clone();
            let callback = callback.clone();

            button.connect_clicked(move |_button| {
                callback(widget_id.clone(), action.clone());
            });
        }

        Self { button }
    }
}

impl crate::widget_base::WidgetBase for MenuRowWidget {
    fn widget(&self) -> gtk::Widget {
        self.button.clone().upcast()
    }
}
