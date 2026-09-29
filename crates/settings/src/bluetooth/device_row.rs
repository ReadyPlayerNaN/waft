//! XML-backed row for a Bluetooth device.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use adw::prelude::*;
use waft_protocol::entity::bluetooth::ConnectionState;
use waft_ui_gtk::bluetooth::resolve_device_type_icon;
use waft_ui_gtk::icons::IconWidget;

use crate::i18n::{t, t_args};

/// Props for creating or updating a device row.
#[derive(Clone, PartialEq)]
pub struct DeviceRowProps {
    pub name: String,
    pub device_type: String,
    pub connection_state: ConnectionState,
    pub paired: bool,
    pub battery_percentage: Option<u8>,
    pub rssi: Option<i16>,
}

/// Output events from a device row.
pub enum DeviceRowOutput {
    ToggleConnect,
    Pair,
    Remove,
}

type OutputCallback = Rc<RefCell<Option<Box<dyn Fn(DeviceRowOutput)>>>>;

/// A Bluetooth device row with XML-defined structure.
pub struct DeviceRow {
    pub root: adw::ActionRow,
    action_button: gtk::Button,
    remove_button: gtk::Button,
    battery_icon_slot: gtk::Box,
    device_icon: IconWidget,
    battery_icon: IconWidget,
    paired: Rc<Cell<bool>>,
    output_cb: OutputCallback,
}

impl DeviceRow {
    pub fn build(props: &DeviceRowProps) -> Self {
        let builder = gtk::Builder::from_resource("/com/waft/settings/device-row.ui");
        let root: adw::ActionRow = builder
            .object("root")
            .expect("device-row.ui must contain root");
        let device_icon_slot: gtk::Box = builder
            .object("device_icon_slot")
            .expect("device-row.ui must contain device_icon_slot");
        let action_button: gtk::Button = builder
            .object("action_button")
            .expect("device-row.ui must contain action_button");
        let remove_button: gtk::Button = builder
            .object("remove_button")
            .expect("device-row.ui must contain remove_button");
        let battery_icon_slot: gtk::Box = builder
            .object("battery_icon_slot")
            .expect("device-row.ui must contain battery_icon_slot");

        let device_icon = IconWidget::from_name("bluetooth-symbolic", 24);
        let battery_icon = IconWidget::from_name("battery-full-symbolic", 16);
        device_icon_slot.append(device_icon.widget());
        battery_icon_slot.append(battery_icon.widget());

        let output_cb: OutputCallback = Rc::new(RefCell::new(None));
        let paired = Rc::new(Cell::new(false));
        {
            let output_cb = output_cb.clone();
            let paired = paired.clone();
            action_button.connect_clicked(move |_| {
                let output = if paired.get() {
                    DeviceRowOutput::ToggleConnect
                } else {
                    DeviceRowOutput::Pair
                };
                if let Some(callback) = output_cb.borrow().as_ref() {
                    callback(output);
                }
            });
        }
        {
            let output_cb = output_cb.clone();
            remove_button.connect_clicked(move |_| {
                if let Some(callback) = output_cb.borrow().as_ref() {
                    callback(DeviceRowOutput::Remove);
                }
            });
        }

        let row = Self {
            root,
            action_button,
            remove_button,
            battery_icon_slot,
            device_icon,
            battery_icon,
            paired,
            output_cb,
        };
        row.update(props);
        row
    }

    pub fn update(&self, props: &DeviceRowProps) {
        let connected = matches!(props.connection_state, ConnectionState::Connected);
        let (subtitle, action_label, sensitive) = if props.paired {
            match props.connection_state {
                ConnectionState::Connected => {
                    let text = if let Some(pct) = props.battery_percentage {
                        t_args("bt-battery-pct", &[("pct", &pct.to_string())])
                    } else {
                        t("bt-connected")
                    };
                    (text, t("bt-disconnect"), true)
                }
                ConnectionState::Connecting => (t("bt-connecting"), t("bt-cancel"), false),
                ConnectionState::Disconnecting => (t("bt-disconnecting"), t("bt-wait"), false),
                ConnectionState::Disconnected => (t("bt-disconnected"), t("bt-connect"), true),
            }
        } else {
            let sub = match props.rssi {
                Some(rssi) if rssi > -50 => t("bt-signal-excellent"),
                Some(rssi) if rssi > -70 => t("bt-signal-good"),
                Some(rssi) if rssi > -85 => t("bt-signal-fair"),
                Some(_) => t("bt-signal-weak"),
                None => String::new(),
            };
            (sub, t("bt-pair"), true)
        };

        self.root.set_title(&props.name);
        self.root.set_subtitle(&subtitle);
        self.device_icon.set_icon(resolve_device_type_icon(&props.device_type));
        self.action_button.set_label(&action_label);
        self.action_button.set_sensitive(sensitive);
        self.remove_button.set_visible(props.paired);
        self.paired.set(props.paired);
        self.action_button.remove_css_class("suggested-action");
        if !props.paired {
            self.action_button.add_css_class("suggested-action");
        }

        let show_battery = props.paired && connected && props.battery_percentage.is_some();
        self.battery_icon_slot.set_visible(show_battery);
        if let Some(pct) = props.battery_percentage {
            self.battery_icon
                .set_icon(resolve_battery_icon_name(pct));
        }
    }

    pub fn connect_output<F: Fn(DeviceRowOutput) + 'static>(&self, callback: F) {
        *self.output_cb.borrow_mut() = Some(Box::new(callback));
    }

    pub fn widget(&self) -> gtk::Widget {
        self.root.clone().upcast()
    }
}

fn resolve_battery_icon_name(pct: u8) -> &'static str {
    match pct {
        0..=10 => "battery-level-0-symbolic",
        11..=30 => "battery-caution-symbolic",
        31..=50 => "battery-level-30-symbolic",
        51..=70 => "battery-level-50-symbolic",
        71..=90 => "battery-level-70-symbolic",
        _ => "battery-full-symbolic",
    }
}
