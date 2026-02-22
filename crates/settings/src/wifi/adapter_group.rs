//! Per-adapter WiFi preferences group.
//!
//! Dumb widget displaying WiFi adapter controls: enable toggle.

use std::cell::RefCell;
use std::rc::Rc;

use adw::prelude::*;
use waft_ui_gtk::vdom::Component;

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

/// Callback type for WiFi adapter group output events.
type OutputCallback = Rc<RefCell<Option<Box<dyn Fn(WifiAdapterGroupOutput)>>>>;

/// Per-adapter WiFi preferences group with controls.
pub struct WifiAdapterGroup {
    pub root: adw::PreferencesGroup,
    enabled_row: adw::SwitchRow,
    enabled: Rc<RefCell<bool>>,
    /// Guard against feedback loops when programmatically updating switch state.
    updating: Rc<RefCell<bool>>,
    output_cb: OutputCallback,
}

impl Component for WifiAdapterGroup {
    type Props = WifiAdapterGroupProps;
    type Output = WifiAdapterGroupOutput;

    fn build(props: &Self::Props) -> Self {
        let group = adw::PreferencesGroup::builder().title(&props.name).build();

        let enabled_row = adw::SwitchRow::builder().title(t("wifi-adapter-enabled")).build();
        group.add(&enabled_row);

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

        let adapter = Self {
            root: group,
            enabled_row,
            enabled,
            updating,
            output_cb,
        };

        adapter.update(props);
        adapter
    }

    fn update(&self, props: &Self::Props) {
        *self.updating.borrow_mut() = true;
        *self.enabled.borrow_mut() = props.enabled;

        self.root.set_title(&props.name);
        self.enabled_row.set_active(props.enabled);

        *self.updating.borrow_mut() = false;
    }

    fn widget(&self) -> gtk::Widget {
        self.root.clone().upcast()
    }

    fn connect_output<F: Fn(Self::Output) + 'static>(&self, callback: F) {
        *self.output_cb.borrow_mut() = Some(Box::new(callback));
    }
}
