//! XML-backed widget for a single systemd user service row.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use adw::prelude::*;

use crate::i18n::t;

/// Input data for constructing or updating a service row.
#[derive(Clone, PartialEq)]
pub struct ServiceRowProps {
    pub unit: String,
    pub description: String,
    pub active_state: String,
    pub enabled: bool,
    pub sub_state: String,
}

/// Output events from a service row.
#[derive(Debug, Clone)]
pub enum ServiceRowOutput {
    Start,
    Stop,
    Enable,
    Disable,
}

type OutputCallback = Rc<RefCell<Option<Box<dyn Fn(ServiceRowOutput)>>>>;

/// A service row whose stable hierarchy is defined in GTK XML.
pub struct ServiceRow {
    pub root: adw::ActionRow,
    state_label: gtk::Label,
    start_stop_button: gtk::Button,
    enable_switch: gtk::Switch,
    running: Rc<Cell<bool>>,
    updating: Rc<Cell<bool>>,
    output_cb: OutputCallback,
}

impl ServiceRow {
    pub fn build(props: &ServiceRowProps) -> Self {
        let builder = gtk::Builder::from_resource("/com/waft/settings/service-row.ui");
        let root: adw::ActionRow = builder
            .object("root")
            .expect("service-row.ui must contain root");
        let state_label: gtk::Label = builder
            .object("state_label")
            .expect("service-row.ui must contain state_label");
        let start_stop_button: gtk::Button = builder
            .object("start_stop_button")
            .expect("service-row.ui must contain start_stop_button");
        let enable_switch: gtk::Switch = builder
            .object("enable_switch")
            .expect("service-row.ui must contain enable_switch");

        let output_cb: OutputCallback = Rc::new(RefCell::new(None));
        let running = Rc::new(Cell::new(false));
        let updating = Rc::new(Cell::new(false));

        {
            let output_cb = output_cb.clone();
            let running = running.clone();
            start_stop_button.connect_clicked(move |_| {
                let output = if running.get() {
                    ServiceRowOutput::Stop
                } else {
                    ServiceRowOutput::Start
                };
                if let Some(callback) = output_cb.borrow().as_ref() {
                    callback(output);
                }
            });
        }
        {
            let output_cb = output_cb.clone();
            let updating = updating.clone();
            enable_switch.connect_active_notify(move |switch| {
                if updating.get() {
                    return;
                }
                let output = if switch.is_active() {
                    ServiceRowOutput::Enable
                } else {
                    ServiceRowOutput::Disable
                };
                if let Some(callback) = output_cb.borrow().as_ref() {
                    callback(output);
                }
            });
        }

        let row = Self {
            root,
            state_label,
            start_stop_button,
            enable_switch,
            running,
            updating,
            output_cb,
        };
        row.update(props);
        row
    }

    pub fn update(&self, props: &ServiceRowProps) {
        let running = props.active_state == "active" || props.active_state == "activating";
        let controllable = props.sub_state != "static" && props.sub_state != "masked";
        let display_name = props.unit.strip_suffix(".service").unwrap_or(&props.unit);
        let subtitle = if props.description.is_empty() {
            props.active_state.clone()
        } else {
            props.description.clone()
        };

        self.root.set_title(display_name);
        self.root.set_subtitle(&subtitle);
        self.state_label.set_label(&props.active_state);
        for class in ["success", "error", "dim-label"] {
            self.state_label.remove_css_class(class);
        }
        self.state_label.add_css_class(match props.active_state.as_str() {
            "active" => "success",
            "failed" => "error",
            _ => "dim-label",
        });
        let button_label = if running {
            t("services-stop")
        } else {
            t("services-start")
        };
        self.start_stop_button.set_label(&button_label);
        self.start_stop_button.set_sensitive(!props.active_state.is_empty());
        self.running.set(running);
        self.updating.set(true);
        self.enable_switch.set_sensitive(controllable);
        self.enable_switch.set_active(props.enabled);
        self.updating.set(false);
    }

    pub fn connect_output<F: Fn(ServiceRowOutput) + 'static>(&self, callback: F) {
        *self.output_cb.borrow_mut() = Some(Box::new(callback));
    }

    pub fn widget(&self) -> gtk::Widget {
        self.root.clone().upcast()
    }
}
