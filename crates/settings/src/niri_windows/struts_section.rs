//! Struts settings section -- dumb widget.
//!
//! Four SpinRows for left, right, top, bottom screen-edge reservations.

use std::cell::RefCell;
use std::rc::Rc;

use adw::prelude::*;

use crate::i18n::t;

/// Output events from the struts section.
pub enum StrutsSectionOutput {
    Left(u32),
    Right(u32),
    Top(u32),
    Bottom(u32),
}

type OutputCallback = Rc<RefCell<Option<Box<dyn Fn(StrutsSectionOutput)>>>>;

/// Struts controls.
pub struct StrutsSection {
    pub root: adw::PreferencesGroup,
    output_cb: OutputCallback,
    left_row: adw::SpinRow,
    right_row: adw::SpinRow,
    top_row: adw::SpinRow,
    bottom_row: adw::SpinRow,
    updating: Rc<std::cell::Cell<bool>>,
}

impl StrutsSection {
    pub fn new() -> Self {
        let output_cb: OutputCallback = Rc::new(RefCell::new(None));
        let updating = Rc::new(std::cell::Cell::new(false));

        let group = adw::PreferencesGroup::builder()
            .title(t("windows-struts"))
            .build();

        let left_adj = gtk::Adjustment::new(0.0, 0.0, 512.0, 1.0, 10.0, 0.0);
        let left_row = adw::SpinRow::builder()
            .title(t("windows-struts-left"))
            .adjustment(&left_adj)
            .build();
        group.add(&left_row);

        let right_adj = gtk::Adjustment::new(0.0, 0.0, 512.0, 1.0, 10.0, 0.0);
        let right_row = adw::SpinRow::builder()
            .title(t("windows-struts-right"))
            .adjustment(&right_adj)
            .build();
        group.add(&right_row);

        let top_adj = gtk::Adjustment::new(0.0, 0.0, 512.0, 1.0, 10.0, 0.0);
        let top_row = adw::SpinRow::builder()
            .title(t("windows-struts-top"))
            .adjustment(&top_adj)
            .build();
        group.add(&top_row);

        let bottom_adj = gtk::Adjustment::new(0.0, 0.0, 512.0, 1.0, 10.0, 0.0);
        let bottom_row = adw::SpinRow::builder()
            .title(t("windows-struts-bottom"))
            .adjustment(&bottom_adj)
            .build();
        group.add(&bottom_row);

        wire_spin(&left_row, &output_cb, &updating, |v| {
            StrutsSectionOutput::Left(v)
        });
        wire_spin(&right_row, &output_cb, &updating, |v| {
            StrutsSectionOutput::Right(v)
        });
        wire_spin(&top_row, &output_cb, &updating, |v| {
            StrutsSectionOutput::Top(v)
        });
        wire_spin(&bottom_row, &output_cb, &updating, |v| {
            StrutsSectionOutput::Bottom(v)
        });

        Self {
            root: group,
            output_cb,
            left_row,
            right_row,
            top_row,
            bottom_row,
            updating,
        }
    }

    pub fn apply_props(&self, left: u32, right: u32, top: u32, bottom: u32) {
        self.updating.set(true);
        self.left_row.set_value(f64::from(left));
        self.right_row.set_value(f64::from(right));
        self.top_row.set_value(f64::from(top));
        self.bottom_row.set_value(f64::from(bottom));
        self.updating.set(false);
    }

    pub fn connect_output<F: Fn(StrutsSectionOutput) + 'static>(&self, callback: F) {
        *self.output_cb.borrow_mut() = Some(Box::new(callback));
    }
}

fn wire_spin<F>(
    row: &adw::SpinRow,
    output_cb: &OutputCallback,
    updating: &Rc<std::cell::Cell<bool>>,
    make_output: F,
) where
    F: Fn(u32) -> StrutsSectionOutput + 'static,
{
    let cb = output_cb.clone();
    let updating_ref = updating.clone();
    row.connect_value_notify(move |row| {
        if updating_ref.get() {
            return;
        }
        if let Some(ref callback) = *cb.borrow() {
            callback(make_output(row.value() as u32));
        }
    });
}
