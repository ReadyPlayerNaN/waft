//! XML-backed WiFi adapter preferences group.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use adw::prelude::*;

use crate::i18n::t;

/// Props for creating or updating a WiFi adapter group.
#[derive(Clone, PartialEq)]
pub struct WifiAdapterGroupProps {
    pub name: String,
    pub enabled: bool,
}

/// Output events from a WiFi adapter group.
pub enum WifiAdapterGroupOutput {
    Enable,
    Disable,
}

type OutputCallback = Rc<RefCell<Option<Box<dyn Fn(WifiAdapterGroupOutput)>>>>;

/// A WiFi adapter group with XML-defined stable structure.
pub struct WifiAdapterGroup {
    pub root: adw::PreferencesGroup,
    enabled_row: adw::SwitchRow,
    updating: Rc<Cell<bool>>,
    output_cb: OutputCallback,
}

impl WifiAdapterGroup {
    pub fn build(props: &WifiAdapterGroupProps) -> Self {
        let builder = gtk::Builder::from_resource("/com/waft/settings/wifi-adapter-group.ui");
        let root: adw::PreferencesGroup = builder
            .object("root")
            .expect("wifi-adapter-group.ui must contain root");
        let enabled_row: adw::SwitchRow = builder
            .object("enabled_row")
            .expect("wifi-adapter-group.ui must contain enabled_row");
        let output_cb: OutputCallback = Rc::new(RefCell::new(None));
        let updating = Rc::new(Cell::new(false));
        enabled_row.set_title(&t("wifi-adapter-enabled"));
        {
            let output_cb = output_cb.clone();
            let updating = updating.clone();
            enabled_row.connect_active_notify(move |row| {
                if updating.get() {
                    return;
                }
                let output = if row.is_active() {
                    WifiAdapterGroupOutput::Enable
                } else {
                    WifiAdapterGroupOutput::Disable
                };
                if let Some(callback) = output_cb.borrow().as_ref() {
                    callback(output);
                }
            });
        }
        let group = Self {
            root,
            enabled_row,
            updating,
            output_cb,
        };
        group.update(props);
        group
    }

    pub fn update(&self, props: &WifiAdapterGroupProps) {
        self.root.set_title(&props.name);
        self.updating.set(true);
        self.enabled_row.set_active(props.enabled);
        self.updating.set(false);
    }

    pub fn connect_output<F: Fn(WifiAdapterGroupOutput) + 'static>(&self, callback: F) {
        *self.output_cb.borrow_mut() = Some(Box::new(callback));
    }

    pub fn widget(&self) -> gtk::Widget {
        self.root.clone().upcast()
    }
}
