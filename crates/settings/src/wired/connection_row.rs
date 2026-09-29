//! XML-backed Ethernet connection profile row.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use adw::prelude::*;
use waft_ui_gtk::icons::IconWidget;

use crate::i18n::t;

/// Props for creating or updating a connection row.
#[derive(Clone, PartialEq)]
pub struct ConnectionRowProps {
    pub name: String,
    pub active: bool,
}

/// Output events from a connection row.
pub enum ConnectionRowOutput {
    Activate,
    Deactivate,
}

type OutputCallback = Rc<RefCell<Option<Box<dyn Fn(ConnectionRowOutput)>>>>;

/// An Ethernet connection row with XML-defined structure.
pub struct WiredConnectionRow {
    pub root: adw::ActionRow,
    action_button: gtk::Button,
    active_icon_slot: gtk::Box,
    _active_icon: IconWidget,
    active: Rc<Cell<bool>>,
    output_cb: OutputCallback,
}

impl WiredConnectionRow {
    pub fn build(props: &ConnectionRowProps) -> Self {
        let builder = gtk::Builder::from_resource("/com/waft/settings/connection-row.ui");
        let root: adw::ActionRow = builder
            .object("root")
            .expect("connection-row.ui must contain root");
        let action_button: gtk::Button = builder
            .object("action_button")
            .expect("connection-row.ui must contain action_button");
        let active_icon_slot: gtk::Box = builder
            .object("active_icon_slot")
            .expect("connection-row.ui must contain active_icon_slot");
        let active_icon = IconWidget::from_name("emblem-default-symbolic", 16);
        active_icon_slot.append(active_icon.widget());
        let active = Rc::new(Cell::new(false));
        let output_cb: OutputCallback = Rc::new(RefCell::new(None));
        {
            let output_cb = output_cb.clone();
            let active = active.clone();
            action_button.connect_clicked(move |_| {
                let output = if active.get() {
                    ConnectionRowOutput::Deactivate
                } else {
                    ConnectionRowOutput::Activate
                };
                if let Some(callback) = output_cb.borrow().as_ref() {
                    callback(output);
                }
            });
        }
        let row = Self {
            root,
            action_button,
            active_icon_slot,
            _active_icon: active_icon,
            active,
            output_cb,
        };
        row.update(props);
        row
    }

    pub fn update(&self, props: &ConnectionRowProps) {
        self.root.set_title(&props.name);
        let subtitle = if props.active {
            t("wired-active")
        } else {
            String::new()
        };
        self.root.set_subtitle(&subtitle);
        let label = if props.active {
            t("wired-disconnect")
        } else {
            t("wired-connect")
        };
        self.action_button.set_label(&label);
        self.active_icon_slot.set_visible(props.active);
        self.active.set(props.active);
    }

    pub fn connect_output<F: Fn(ConnectionRowOutput) + 'static>(&self, callback: F) {
        *self.output_cb.borrow_mut() = Some(Box::new(callback));
    }

    pub fn widget(&self) -> gtk::Widget {
        self.root.clone().upcast()
    }
}
