//! XML-backed Bluetooth adapter preferences group.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use adw::prelude::*;

use crate::i18n::t;

/// Props for creating or updating an adapter group.
#[derive(Clone, PartialEq)]
pub struct AdapterGroupProps {
    pub name: String,
    pub powered: bool,
    pub discoverable: bool,
}

/// Output events from an adapter group.
pub enum AdapterGroupOutput {
    TogglePower,
    ToggleDiscoverable,
    SetAlias(String),
}

type OutputCallback = Rc<RefCell<Option<Box<dyn Fn(AdapterGroupOutput)>>>>;

/// A Bluetooth adapter group with XML-defined static structure.
pub struct AdapterGroup {
    pub root: adw::PreferencesGroup,
    power_row: adw::SwitchRow,
    discoverable_row: adw::SwitchRow,
    alias_row: adw::EntryRow,
    updating: Rc<Cell<bool>>,
    output_cb: OutputCallback,
}

impl AdapterGroup {
    pub fn build(props: &AdapterGroupProps) -> Self {
        let builder = gtk::Builder::from_resource("/com/waft/settings/bluetooth-adapter-group.ui");
        let root: adw::PreferencesGroup = builder
            .object("root")
            .expect("bluetooth-adapter-group.ui must contain root");
        let power_row: adw::SwitchRow = builder
            .object("power_row")
            .expect("bluetooth-adapter-group.ui must contain power_row");
        let discoverable_row: adw::SwitchRow = builder
            .object("discoverable_row")
            .expect("bluetooth-adapter-group.ui must contain discoverable_row");
        let alias_row: adw::EntryRow = builder
            .object("alias_row")
            .expect("bluetooth-adapter-group.ui must contain alias_row");
        let output_cb: OutputCallback = Rc::new(RefCell::new(None));
        let updating = Rc::new(Cell::new(false));

        power_row.set_title(&t("bt-adapter-enabled"));
        discoverable_row.set_title(&t("bt-adapter-discoverable"));
        alias_row.set_title(&t("bt-adapter-device-name"));
        {
            let output_cb = output_cb.clone();
            let updating = updating.clone();
            power_row.connect_active_notify(move |_| {
                if updating.get() {
                    return;
                }
                if let Some(callback) = output_cb.borrow().as_ref() {
                    callback(AdapterGroupOutput::TogglePower);
                }
            });
        }
        {
            let output_cb = output_cb.clone();
            let updating = updating.clone();
            discoverable_row.connect_active_notify(move |_| {
                if updating.get() {
                    return;
                }
                if let Some(callback) = output_cb.borrow().as_ref() {
                    callback(AdapterGroupOutput::ToggleDiscoverable);
                }
            });
        }
        {
            let output_cb = output_cb.clone();
            let updating = updating.clone();
            alias_row.connect_apply(move |row| {
                if updating.get() || row.text().is_empty() {
                    return;
                }
                if let Some(callback) = output_cb.borrow().as_ref() {
                    callback(AdapterGroupOutput::SetAlias(row.text().to_string()));
                }
            });
        }

        let group = Self {
            root,
            power_row,
            discoverable_row,
            alias_row,
            updating,
            output_cb,
        };
        group.update(props);
        group
    }

    pub fn update(&self, props: &AdapterGroupProps) {
        self.root.set_title(&props.name);
        self.updating.set(true);
        self.power_row.set_active(props.powered);
        self.discoverable_row.set_active(props.discoverable);
        self.discoverable_row.set_sensitive(props.powered);
        self.alias_row.set_text(&props.name);
        self.alias_row.set_sensitive(props.powered);
        self.updating.set(false);
    }

    pub fn connect_output<F: Fn(AdapterGroupOutput) + 'static>(&self, callback: F) {
        *self.output_cb.borrow_mut() = Some(Box::new(callback));
    }

    pub fn widget(&self) -> gtk::Widget {
        self.root.clone().upcast()
    }
}
