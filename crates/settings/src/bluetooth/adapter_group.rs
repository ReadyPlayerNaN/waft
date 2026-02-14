//! Per-adapter preferences group.
//!
//! Dumb widget displaying adapter controls: power toggle, discoverable toggle,
//! device name entry, and discovery scan button.

use std::cell::RefCell;
use std::rc::Rc;

use adw::prelude::*;

/// Props for creating or updating an adapter group.
pub struct AdapterGroupProps {
    pub name: String,
    pub powered: bool,
    pub discoverable: bool,
    pub discovering: bool,
}

/// Output events from an adapter group.
pub enum AdapterGroupOutput {
    /// Toggle adapter power on/off.
    TogglePower,
    /// Toggle adapter discoverability.
    ToggleDiscoverable,
    /// Set adapter alias/name.
    SetAlias(String),
    /// Start device discovery scanning.
    StartDiscovery,
    /// Stop device discovery scanning.
    StopDiscovery,
}

/// Callback type for adapter group output events.
type OutputCallback = Rc<RefCell<Option<Box<dyn Fn(AdapterGroupOutput)>>>>;

/// Per-adapter preferences group with controls.
pub struct AdapterGroup {
    pub root: adw::PreferencesGroup,
    power_row: adw::SwitchRow,
    discoverable_row: adw::SwitchRow,
    alias_row: adw::EntryRow,
    scan_button: gtk::Button,
    /// Current discovery state, used to decide start vs stop action.
    discovering: Rc<RefCell<bool>>,
    /// Guard against feedback loops when programmatically updating switch state.
    updating: Rc<RefCell<bool>>,
    output_cb: OutputCallback,
}

impl AdapterGroup {
    pub fn new(props: &AdapterGroupProps) -> Self {
        let group = adw::PreferencesGroup::builder()
            .title(&props.name)
            .build();

        // Power switch
        let power_row = adw::SwitchRow::builder()
            .title("Enabled")
            .build();
        group.add(&power_row);

        // Discoverable switch
        let discoverable_row = adw::SwitchRow::builder()
            .title("Discoverable")
            .build();
        group.add(&discoverable_row);

        // Alias entry
        let alias_row = adw::EntryRow::builder()
            .title("Device Name")
            .text(&props.name)
            .show_apply_button(true)
            .build();
        group.add(&alias_row);

        // Scan button
        let scan_button = gtk::Button::builder()
            .halign(gtk::Align::Start)
            .css_classes(["pill"])
            .margin_top(12)
            .build();
        group.add(&scan_button);

        let discovering = Rc::new(RefCell::new(props.discovering));
        let updating = Rc::new(RefCell::new(false));
        let output_cb: OutputCallback = Rc::new(RefCell::new(None));

        // Wire power toggle
        let cb = output_cb.clone();
        let guard = updating.clone();
        power_row.connect_active_notify(move |_row| {
            if *guard.borrow() {
                return;
            }
            if let Some(ref callback) = *cb.borrow() {
                callback(AdapterGroupOutput::TogglePower);
            }
        });

        // Wire discoverable toggle
        let cb = output_cb.clone();
        let guard = updating.clone();
        discoverable_row.connect_active_notify(move |_row| {
            if *guard.borrow() {
                return;
            }
            if let Some(ref callback) = *cb.borrow() {
                callback(AdapterGroupOutput::ToggleDiscoverable);
            }
        });

        // Wire alias apply
        let cb = output_cb.clone();
        alias_row.connect_apply(move |row| {
            let text = row.text().to_string();
            if !text.is_empty()
                && let Some(ref callback) = *cb.borrow()
            {
                callback(AdapterGroupOutput::SetAlias(text));
            }
        });

        // Wire scan button
        let cb = output_cb.clone();
        let disc = discovering.clone();
        scan_button.connect_clicked(move |_| {
            if let Some(ref callback) = *cb.borrow() {
                if *disc.borrow() {
                    callback(AdapterGroupOutput::StopDiscovery);
                } else {
                    callback(AdapterGroupOutput::StartDiscovery);
                }
            }
        });

        let adapter_group = Self {
            root: group,
            power_row,
            discoverable_row,
            alias_row,
            scan_button,
            discovering,
            updating,
            output_cb,
        };

        adapter_group.apply_props(props);
        adapter_group
    }

    /// Update the group to reflect new adapter state.
    pub fn apply_props(&self, props: &AdapterGroupProps) {
        *self.updating.borrow_mut() = true;
        *self.discovering.borrow_mut() = props.discovering;

        self.root.set_title(&props.name);
        self.power_row.set_active(props.powered);
        self.discoverable_row.set_active(props.discoverable);
        self.discoverable_row.set_sensitive(props.powered);
        self.alias_row.set_text(&props.name);
        self.alias_row.set_sensitive(props.powered);

        if props.discovering {
            self.scan_button.set_label("Stop Scanning");
            self.scan_button.add_css_class("destructive-action");
            self.scan_button.remove_css_class("suggested-action");
        } else {
            self.scan_button.set_label("Start Scanning");
            self.scan_button.add_css_class("suggested-action");
            self.scan_button.remove_css_class("destructive-action");
        }
        self.scan_button.set_sensitive(props.powered);

        *self.updating.borrow_mut() = false;
    }

    /// Register a callback for adapter group output events.
    pub fn connect_output<F: Fn(AdapterGroupOutput) + 'static>(&self, callback: F) {
        *self.output_cb.borrow_mut() = Some(Box::new(callback));
    }
}
