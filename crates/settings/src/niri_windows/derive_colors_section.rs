//! Derive colours settings section -- dumb widget.
//!
//! Single SwitchRow that controls whether focus ring, border, and tab indicator
//! colours are derived from the GTK accent colour. When unavailable (no
//! gtk-appearance entity), the switch is insensitive with an explanatory subtitle.

use std::cell::RefCell;
use std::rc::Rc;

use adw::prelude::*;

use crate::i18n::t;

/// Output events from the derive colours section.
pub enum DeriveColorsSectionOutput {
    Toggled(bool),
}

type OutputCallback = Rc<RefCell<Option<Box<dyn Fn(DeriveColorsSectionOutput)>>>>;

/// Derive colours control.
pub struct DeriveColorsSection {
    pub root: adw::PreferencesGroup,
    output_cb: OutputCallback,
    switch_row: adw::SwitchRow,
    updating: Rc<std::cell::Cell<bool>>,
}

impl DeriveColorsSection {
    pub fn new() -> Self {
        let output_cb: OutputCallback = Rc::new(RefCell::new(None));
        let updating = Rc::new(std::cell::Cell::new(false));

        let group = adw::PreferencesGroup::builder().build();

        let switch_row = adw::SwitchRow::builder()
            .title(t("windows-derive-colors"))
            .subtitle(t("windows-derive-colors-sub"))
            .build();
        group.add(&switch_row);

        {
            let cb = output_cb.clone();
            let updating_ref = updating.clone();
            switch_row.connect_active_notify(move |row| {
                if updating_ref.get() {
                    return;
                }
                if let Some(ref callback) = *cb.borrow() {
                    callback(DeriveColorsSectionOutput::Toggled(row.is_active()));
                }
            });
        }

        Self {
            root: group,
            output_cb,
            switch_row,
            updating,
        }
    }

    /// Set whether the GTK appearance plugin is available.
    /// When unavailable, the switch is insensitive with an explanatory subtitle.
    pub fn set_available(&self, available: bool) {
        self.switch_row.set_sensitive(available);
        if available {
            self.switch_row
                .set_subtitle(&t("windows-derive-colors-sub"));
        } else {
            self.switch_row
                .set_subtitle(&t("windows-derive-colors-unavailable"));
        }
    }

    /// Set the toggle state without firing the output callback.
    pub fn set_active(&self, active: bool) {
        self.updating.set(true);
        self.switch_row.set_active(active);
        self.updating.set(false);
    }

    pub fn connect_output<F: Fn(DeriveColorsSectionOutput) + 'static>(&self, callback: F) {
        *self.output_cb.borrow_mut() = Some(Box::new(callback));
    }
}
