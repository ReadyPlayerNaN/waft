//! Switch widget

use crate::types::ActionCallback;
use waft_ipc::widget::Action;
use gtk::prelude::*;

/// GTK4 switch widget with active state, sensitivity, and toggle action.
pub struct SwitchWidget {
    switch: gtk::Switch,
}

impl SwitchWidget {
    pub fn new(
        callback: &ActionCallback,
        active: bool,
        sensitive: bool,
        on_toggle: &Action,
        widget_id: &str,
    ) -> Self {
        let switch = gtk::Switch::new();
        switch.set_active(active);
        switch.set_sensitive(sensitive);
        switch.set_valign(gtk::Align::Center);

        let widget_id = widget_id.to_string();
        let on_toggle = on_toggle.clone();
        let callback = callback.clone();

        switch.connect_state_set(move |_switch, state| {
            let mut action = on_toggle.clone();
            action.params = crate::types::ActionParams::Value(if state { 1.0 } else { 0.0 });
            callback(widget_id.clone(), action);
            gtk::glib::Propagation::Proceed
        });

        Self { switch }
    }

    pub fn set_active(&self, active: bool) {
        self.switch.set_active(active);
    }

    pub fn set_sensitive(&self, sensitive: bool) {
        self.switch.set_sensitive(sensitive);
    }
}

impl crate::widget_base::WidgetBase for SwitchWidget {
    fn widget(&self) -> gtk::Widget {
        self.switch.clone().upcast()
    }
}
