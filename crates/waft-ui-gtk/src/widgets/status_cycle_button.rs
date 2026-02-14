//! StatusCycleButton widget — displays current option and cycles to next on click.

use std::rc::Rc;

use gtk::prelude::*;

use crate::widgets::icon::IconWidget;

/// An option for StatusCycleButton.
#[derive(Clone, Debug)]
pub struct StatusOption {
    pub id: String,
    pub label: String,
}

/// Callback type for cycle events — receives the next option ID.
pub type CycleCallback = Rc<dyn Fn(String)>;

/// GTK4 status cycle button widget.
#[derive(Clone)]
pub struct StatusCycleButtonWidget {
    root: gtk::Button,
    icon_widget: IconWidget,
    label: gtk::Label,
    value: std::cell::RefCell<String>,
    options: std::cell::RefCell<Vec<StatusOption>>,
}

impl StatusCycleButtonWidget {
    pub fn new(value: &str, icon: &str, options: &[StatusOption], on_cycle: CycleCallback) -> Self {
        let content = gtk::Box::builder()
            .orientation(gtk::Orientation::Horizontal)
            .spacing(8)
            .build();

        let icon_widget = IconWidget::from_name(icon, 16);

        let display_label = Self::find_label(value, options);
        let label = gtk::Label::new(Some(&display_label));

        content.append(icon_widget.widget());
        content.append(&label);

        let root = gtk::Button::builder()
            .css_classes(["flat", "status-cycle-button"])
            .child(&content)
            .sensitive(options.len() >= 2)
            .build();

        let opts = options.to_vec();
        let current_value = value.to_string();
        root.connect_clicked(move |_| {
            let next_id = Self::next_option_id(&current_value, &opts);
            on_cycle(next_id);
        });

        Self {
            root,
            icon_widget,
            label,
            value: std::cell::RefCell::new(value.to_string()),
            options: std::cell::RefCell::new(options.to_vec()),
        }
    }

    /// Find the label for the given value, or "---" if not found.
    fn find_label(value: &str, options: &[StatusOption]) -> String {
        options
            .iter()
            .find(|o| o.id == value)
            .map(|o| o.label.clone())
            .unwrap_or_else(|| "---".to_string())
    }

    /// Get the next option ID after the current value.
    /// If value not found in options, returns the first option's ID.
    fn next_option_id(value: &str, options: &[StatusOption]) -> String {
        if options.is_empty() {
            return String::new();
        }
        let current_idx = options.iter().position(|o| o.id == value);
        match current_idx {
            Some(idx) => {
                let next_idx = (idx + 1) % options.len();
                options[next_idx].id.clone()
            }
            None => options[0].id.clone(),
        }
    }

    /// Update value and options in-place.
    pub fn set_value(&self, value: &str) {
        *self.value.borrow_mut() = value.to_string();
        let opts = self.options.borrow();
        self.label.set_label(&Self::find_label(value, &opts));
    }

    /// Update the icon.
    pub fn set_icon(&self, icon: &str) {
        self.icon_widget.set_icon(icon);
    }

    /// Update options and refresh the label.
    pub fn set_options(&self, options: &[StatusOption]) {
        *self.options.borrow_mut() = options.to_vec();
        let value = self.value.borrow().clone();
        self.label.set_label(&Self::find_label(&value, options));
        self.root.set_sensitive(options.len() >= 2);
    }

    /// Get a reference to the root widget.
    pub fn widget(&self) -> gtk::Widget {
        self.root.clone().upcast()
    }
}

impl crate::widget_base::WidgetBase for StatusCycleButtonWidget {
    fn widget(&self) -> gtk::Widget {
        self.widget()
    }
}
