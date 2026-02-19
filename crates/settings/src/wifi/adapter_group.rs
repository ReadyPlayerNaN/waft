//! Per-adapter WiFi preferences group.
//!
//! Dumb widget displaying WiFi adapter controls: enable toggle and scan button.

use std::cell::RefCell;
use std::rc::Rc;

use adw::prelude::*;

use crate::i18n::t;

/// Props for creating or updating a WiFi adapter group.
pub struct WifiAdapterGroupProps {
    pub name: String,
    pub enabled: bool,
}

/// Output events from a WiFi adapter group.
pub enum WifiAdapterGroupOutput {
    Enable,
    Disable,
    Scan,
}

/// Callback type for WiFi adapter group output events.
type OutputCallback = Rc<RefCell<Option<Box<dyn Fn(WifiAdapterGroupOutput)>>>>;

/// Per-adapter WiFi preferences group with controls.
pub struct WifiAdapterGroup {
    pub root: adw::PreferencesGroup,
    enabled_row: adw::SwitchRow,
    scan_button: gtk::Button,
    enabled: Rc<RefCell<bool>>,
    /// Guard against feedback loops when programmatically updating switch state.
    updating: Rc<RefCell<bool>>,
    output_cb: OutputCallback,
}

impl WifiAdapterGroup {
    pub fn new(props: &WifiAdapterGroupProps) -> Self {
        let group = adw::PreferencesGroup::builder().title(&props.name).build();

        let enabled_row = adw::SwitchRow::builder().title(t("wifi-adapter-enabled")).build();
        group.add(&enabled_row);

        let scan_button = gtk::Button::builder()
            .halign(gtk::Align::Start)
            .css_classes(["pill", "suggested-action"])
            .margin_top(12)
            .build();
        group.add(&scan_button);

        let enabled = Rc::new(RefCell::new(props.enabled));
        let updating = Rc::new(RefCell::new(false));
        let output_cb: OutputCallback = Rc::new(RefCell::new(None));

        // Wire enable toggle
        let cb = output_cb.clone();
        let guard = updating.clone();
        let en = enabled.clone();
        enabled_row.connect_active_notify(move |_row| {
            if *guard.borrow() {
                return;
            }
            if let Some(ref callback) = *cb.borrow() {
                if *en.borrow() {
                    callback(WifiAdapterGroupOutput::Disable);
                } else {
                    callback(WifiAdapterGroupOutput::Enable);
                }
            }
        });

        // Wire scan button
        let cb = output_cb.clone();
        scan_button.connect_clicked(move |_| {
            if let Some(ref callback) = *cb.borrow() {
                callback(WifiAdapterGroupOutput::Scan);
            }
        });

        let adapter = Self {
            root: group,
            enabled_row,
            scan_button,
            enabled,
            updating,
            output_cb,
        };

        adapter.apply_props(props);
        adapter
    }

    /// Update the group to reflect new adapter state.
    pub fn apply_props(&self, props: &WifiAdapterGroupProps) {
        *self.updating.borrow_mut() = true;
        *self.enabled.borrow_mut() = props.enabled;

        self.root.set_title(&props.name);
        self.enabled_row.set_active(props.enabled);
        self.scan_button.set_label(&t("wifi-adapter-scan"));
        self.scan_button.set_sensitive(props.enabled);

        *self.updating.borrow_mut() = false;
    }

    /// Register a callback for WiFi adapter group output events.
    pub fn connect_output<F: Fn(WifiAdapterGroupOutput) + 'static>(&self, callback: F) {
        *self.output_cb.borrow_mut() = Some(Box::new(callback));
    }
}
