//! Per-network row widget.
//!
//! Dumb widget displaying a single WiFi network as an `AdwActionRow`
//! with signal strength icon, security indicator, and connect button.

use std::cell::RefCell;
use std::rc::Rc;

use adw::prelude::*;
use waft_ui_gtk::icons::IconWidget;

use crate::i18n::t;

/// Props for creating or updating a network row.
pub struct NetworkRowProps {
    pub ssid: String,
    pub strength: u8,
    pub secure: bool,
    pub connected: bool,
}

/// Output events from a network row.
pub enum NetworkRowOutput {
    Connect,
    Disconnect,
}

/// Callback type for network row output events.
type OutputCallback = Rc<RefCell<Option<Box<dyn Fn(NetworkRowOutput)>>>>;

fn signal_icon_name(strength: u8) -> &'static str {
    if strength > 75 {
        "network-wireless-signal-excellent-symbolic"
    } else if strength > 50 {
        "network-wireless-signal-good-symbolic"
    } else if strength > 25 {
        "network-wireless-signal-ok-symbolic"
    } else {
        "network-wireless-signal-weak-symbolic"
    }
}

/// A single WiFi network row.
pub struct NetworkRow {
    pub root: adw::ActionRow,
    signal_icon: IconWidget,
    secure_icon: IconWidget,
    action_button: gtk::Button,
    connected: Rc<RefCell<bool>>,
    output_cb: OutputCallback,
}

impl NetworkRow {
    pub fn new(props: &NetworkRowProps) -> Self {
        let signal_icon = IconWidget::from_name(signal_icon_name(props.strength), 16);
        let secure_icon = IconWidget::from_name("channel-secure-symbolic", 16);

        let row = adw::ActionRow::builder().title(&props.ssid).build();

        row.add_prefix(signal_icon.widget());
        row.add_suffix(secure_icon.widget());

        let action_button = gtk::Button::builder()
            .valign(gtk::Align::Center)
            .css_classes(["flat"])
            .build();
        row.add_suffix(&action_button);

        let connected = Rc::new(RefCell::new(props.connected));
        let output_cb: OutputCallback = Rc::new(RefCell::new(None));

        let cb = output_cb.clone();
        let conn = connected.clone();
        action_button.connect_clicked(move |_| {
            if let Some(ref callback) = *cb.borrow() {
                if *conn.borrow() {
                    callback(NetworkRowOutput::Disconnect);
                } else {
                    callback(NetworkRowOutput::Connect);
                }
            }
        });

        let network_row = Self {
            root: row,
            signal_icon,
            secure_icon,
            action_button,
            connected,
            output_cb,
        };

        network_row.apply_props(props);
        network_row
    }

    /// Update the row to reflect new network state.
    pub fn apply_props(&self, props: &NetworkRowProps) {
        *self.connected.borrow_mut() = props.connected;
        self.root.set_title(&props.ssid);
        self.signal_icon.set_icon(signal_icon_name(props.strength));
        self.secure_icon.widget().set_visible(props.secure);

        if props.connected {
            self.root.set_subtitle(&t("wifi-connected"));
            self.action_button.set_label(&t("wifi-disconnect"));
        } else {
            self.root.set_subtitle("");
            self.action_button.set_label(&t("wifi-connect"));
        }
    }

    /// Register a callback for network row output events.
    pub fn connect_output<F: Fn(NetworkRowOutput) + 'static>(&self, callback: F) {
        *self.output_cb.borrow_mut() = Some(Box::new(callback));
    }
}
