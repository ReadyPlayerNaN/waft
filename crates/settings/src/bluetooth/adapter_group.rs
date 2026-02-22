//! Per-adapter preferences group.
//!
//! Dumb widget displaying adapter controls: power toggle, discoverable toggle,
//! and device name entry.

use std::cell::RefCell;
use std::rc::Rc;

use adw::prelude::*;
use waft_ui_gtk::vdom::Component;

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
    /// Toggle adapter power on/off.
    TogglePower,
    /// Toggle adapter discoverability.
    ToggleDiscoverable,
    /// Set adapter alias/name.
    SetAlias(String),
}

/// Callback type for adapter group output events.
type OutputCallback = Rc<RefCell<Option<Box<dyn Fn(AdapterGroupOutput)>>>>;

/// Per-adapter preferences group with controls.
pub struct AdapterGroup {
    pub root: adw::PreferencesGroup,
    power_row: adw::SwitchRow,
    discoverable_row: adw::SwitchRow,
    alias_row: adw::EntryRow,
    /// Guard against feedback loops when programmatically updating switch state.
    updating: Rc<RefCell<bool>>,
    output_cb: OutputCallback,
}

impl Component for AdapterGroup {
    type Props = AdapterGroupProps;
    type Output = AdapterGroupOutput;

    fn build(props: &Self::Props) -> Self {
        let group = adw::PreferencesGroup::builder().title(&props.name).build();

        // Power switch
        let power_row = adw::SwitchRow::builder().title(t("bt-adapter-enabled")).build();
        group.add(&power_row);

        // Discoverable switch
        let discoverable_row = adw::SwitchRow::builder().title(t("bt-adapter-discoverable")).build();
        group.add(&discoverable_row);

        // Alias entry
        let alias_row = adw::EntryRow::builder()
            .title(t("bt-adapter-device-name"))
            .text(&props.name)
            .show_apply_button(true)
            .build();
        group.add(&alias_row);

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

        let adapter_group = Self {
            root: group,
            power_row,
            discoverable_row,
            alias_row,
            updating,
            output_cb,
        };

        adapter_group.update(props);
        adapter_group
    }

    fn update(&self, props: &Self::Props) {
        *self.updating.borrow_mut() = true;

        self.root.set_title(&props.name);
        self.power_row.set_active(props.powered);
        self.discoverable_row.set_active(props.discoverable);
        self.discoverable_row.set_sensitive(props.powered);
        self.alias_row.set_text(&props.name);
        self.alias_row.set_sensitive(props.powered);

        *self.updating.borrow_mut() = false;
    }

    fn widget(&self) -> gtk::Widget {
        self.root.clone().upcast()
    }

    fn connect_output<F: Fn(Self::Output) + 'static>(&self, callback: F) {
        *self.output_cb.borrow_mut() = Some(Box::new(callback));
    }
}
