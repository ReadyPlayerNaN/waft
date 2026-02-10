//! StatusCycleButton widget — displays current option and cycles to next on click.

use gtk::prelude::*;

use crate::reconcile::{ReconcileOutcome, Reconcilable};
use crate::renderer::ActionCallback;
use crate::widgets::icon::IconWidget;
use waft_ipc::widget::{Action, ActionParams, StatusOption};
use waft_ipc::Widget as IpcWidget;

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
    pub fn new(
        value: &str,
        icon: &str,
        options: &[StatusOption],
        callback: &ActionCallback,
        on_cycle: &Action,
        widget_id: &str,
    ) -> Self {
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

        let cb = callback.clone();
        let wid = widget_id.to_string();
        let action = on_cycle.clone();
        let opts = options.to_vec();
        let current_value = value.to_string();
        root.connect_clicked(move |_| {
            let next_id = Self::next_option_id(&current_value, &opts);
            let mut a = action.clone();
            a.params = ActionParams::String(next_id);
            cb(wid.clone(), a);
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

impl Reconcilable for StatusCycleButtonWidget {
    fn try_reconcile(&self, old_desc: &IpcWidget, new_desc: &IpcWidget) -> ReconcileOutcome {
        match (old_desc, new_desc) {
            (
                IpcWidget::StatusCycleButton {
                    on_cycle: old_cycle,
                    ..
                },
                IpcWidget::StatusCycleButton {
                    value,
                    icon,
                    options,
                    on_cycle: new_cycle,
                },
            ) => {
                if old_cycle != new_cycle {
                    return ReconcileOutcome::Recreate;
                }
                self.set_value(value);
                self.set_icon(icon);
                self.set_options(options);
                ReconcileOutcome::Updated
            }
            _ => ReconcileOutcome::Recreate,
        }
    }
}

/// Render a StatusCycleButton from the IPC protocol.
pub(crate) fn render_status_cycle_button(
    callback: &ActionCallback,
    value: &str,
    icon: &str,
    options: &[StatusOption],
    on_cycle: &Action,
    widget_id: &str,
) -> gtk::Widget {
    let widget = StatusCycleButtonWidget::new(value, icon, options, callback, on_cycle, widget_id);
    widget.widget()
}
