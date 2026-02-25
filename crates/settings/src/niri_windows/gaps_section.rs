//! Gaps settings section -- dumb widget.
//!
//! Single SpinRow controlling the gap size between windows.

use std::cell::RefCell;
use std::rc::Rc;

use adw::prelude::*;

use crate::i18n::t;

/// Output events from the gaps section.
pub enum GapsSectionOutput {
    GapsChanged(u32),
}

type OutputCallback = Rc<RefCell<Option<Box<dyn Fn(GapsSectionOutput)>>>>;

/// Gaps control.
pub struct GapsSection {
    pub root: adw::PreferencesGroup,
    output_cb: OutputCallback,
    gaps_row: adw::SpinRow,
    updating: Rc<std::cell::Cell<bool>>,
}

impl GapsSection {
    pub fn new() -> Self {
        let output_cb: OutputCallback = Rc::new(RefCell::new(None));
        let updating = Rc::new(std::cell::Cell::new(false));

        let group = adw::PreferencesGroup::builder()
            .title(t("windows-gaps"))
            .build();

        let gaps_adj = gtk::Adjustment::new(16.0, 0.0, 64.0, 1.0, 4.0, 0.0);
        let gaps_row = adw::SpinRow::builder()
            .title(t("windows-gaps"))
            .adjustment(&gaps_adj)
            .build();
        group.add(&gaps_row);

        {
            let cb = output_cb.clone();
            let updating_ref = updating.clone();
            gaps_row.connect_value_notify(move |row| {
                if updating_ref.get() {
                    return;
                }
                if let Some(ref callback) = *cb.borrow() {
                    callback(GapsSectionOutput::GapsChanged(row.value() as u32));
                }
            });
        }

        Self {
            root: group,
            output_cb,
            gaps_row,
            updating,
        }
    }

    pub fn apply_props(&self, gaps: u32) {
        self.updating.set(true);
        self.gaps_row.set_value(f64::from(gaps));
        self.updating.set(false);
    }

    pub fn connect_output<F: Fn(GapsSectionOutput) + 'static>(&self, callback: F) {
        *self.output_cb.borrow_mut() = Some(Box::new(callback));
    }
}
