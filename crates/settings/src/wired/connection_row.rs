//! Per-connection row widget.
//!
//! Dumb widget displaying a single Ethernet connection profile
//! as an `AdwActionRow` with active indicator and action button.

use std::cell::RefCell;
use std::rc::Rc;

use adw::prelude::*;
use waft_ui_gtk::widgets::icon::IconWidget;

/// Props for creating or updating a connection row.
pub struct ConnectionRowProps {
    pub name: String,
    pub active: bool,
}

/// Output events from a connection row.
pub enum ConnectionRowOutput {
    /// Activate this connection profile.
    Activate,
    /// Deactivate this connection profile.
    Deactivate,
}

/// Callback type for connection row output events.
type OutputCallback = Rc<RefCell<Option<Box<dyn Fn(ConnectionRowOutput)>>>>;

/// A single Ethernet connection profile row.
pub struct WiredConnectionRow {
    pub root: adw::ActionRow,
    check_icon: IconWidget,
    action_button: gtk::Button,
    active: Rc<RefCell<bool>>,
    output_cb: OutputCallback,
}

impl WiredConnectionRow {
    pub fn new(props: &ConnectionRowProps) -> Self {
        let check_icon = IconWidget::from_name("object-select-symbolic", 16);

        let row = adw::ActionRow::builder().title(&props.name).build();

        row.add_suffix(check_icon.widget());

        let action_button = gtk::Button::builder()
            .valign(gtk::Align::Center)
            .css_classes(["flat"])
            .build();
        row.add_suffix(&action_button);

        let active = Rc::new(RefCell::new(props.active));
        let output_cb: OutputCallback = Rc::new(RefCell::new(None));

        let cb = output_cb.clone();
        let act = active.clone();
        action_button.connect_clicked(move |_| {
            if let Some(ref callback) = *cb.borrow() {
                if *act.borrow() {
                    callback(ConnectionRowOutput::Deactivate);
                } else {
                    callback(ConnectionRowOutput::Activate);
                }
            }
        });

        let connection_row = Self {
            root: row,
            check_icon,
            action_button,
            active,
            output_cb,
        };

        connection_row.apply_props(props);
        connection_row
    }

    /// Update the row to reflect new connection state.
    pub fn apply_props(&self, props: &ConnectionRowProps) {
        *self.active.borrow_mut() = props.active;
        self.root.set_title(&props.name);
        self.check_icon.widget().set_visible(props.active);

        if props.active {
            self.root.set_subtitle("Active");
            self.action_button.set_label("Disconnect");
        } else {
            self.root.set_subtitle("");
            self.action_button.set_label("Connect");
        }
    }

    /// Register a callback for connection row output events.
    pub fn connect_output<F: Fn(ConnectionRowOutput) + 'static>(&self, callback: F) {
        *self.output_cb.borrow_mut() = Some(Box::new(callback));
    }
}
