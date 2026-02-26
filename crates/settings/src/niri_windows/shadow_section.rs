//! Shadow settings section -- dumb widget.
//!
//! Controls for niri window shadow: enabled toggle, softness, spread,
//! offset x/y, and two colour pickers (active, inactive) with alpha support.

use std::cell::RefCell;
use std::rc::Rc;

use adw::prelude::*;

use crate::i18n::t;

/// Output events from the shadow section.
pub enum ShadowSectionOutput {
    Toggled(bool),
    SoftnessChanged(u32),
    SpreadChanged(u32),
    OffsetXChanged(i32),
    OffsetYChanged(i32),
    ColorChanged(String),
    InactiveColorChanged(String),
}

type OutputCallback = Rc<RefCell<Option<Box<dyn Fn(ShadowSectionOutput)>>>>;

/// Shadow controls.
pub struct ShadowSection {
    pub root: adw::PreferencesGroup,
    output_cb: OutputCallback,
    switch_row: adw::SwitchRow,
    softness_row: adw::SpinRow,
    spread_row: adw::SpinRow,
    offset_x_row: adw::SpinRow,
    offset_y_row: adw::SpinRow,
    color_btn: gtk::ColorDialogButton,
    inactive_color_btn: gtk::ColorDialogButton,
    updating: Rc<std::cell::Cell<bool>>,
}

impl ShadowSection {
    pub fn new() -> Self {
        let output_cb: OutputCallback = Rc::new(RefCell::new(None));
        let updating = Rc::new(std::cell::Cell::new(false));

        let group = adw::PreferencesGroup::builder()
            .title(t("windows-shadow"))
            .build();

        let switch_row = adw::SwitchRow::builder()
            .title(t("windows-enabled"))
            .build();
        group.add(&switch_row);

        let softness_adj = gtk::Adjustment::new(30.0, 0.0, 100.0, 1.0, 10.0, 0.0);
        let softness_row = adw::SpinRow::builder()
            .title(t("windows-softness"))
            .adjustment(&softness_adj)
            .build();
        group.add(&softness_row);

        let spread_adj = gtk::Adjustment::new(5.0, 0.0, 100.0, 1.0, 10.0, 0.0);
        let spread_row = adw::SpinRow::builder()
            .title(t("windows-spread"))
            .adjustment(&spread_adj)
            .build();
        group.add(&spread_row);

        let offset_x_adj = gtk::Adjustment::new(0.0, -50.0, 50.0, 1.0, 5.0, 0.0);
        let offset_x_row = adw::SpinRow::builder()
            .title(t("windows-offset-x"))
            .adjustment(&offset_x_adj)
            .build();
        group.add(&offset_x_row);

        let offset_y_adj = gtk::Adjustment::new(0.0, -50.0, 50.0, 1.0, 5.0, 0.0);
        let offset_y_row = adw::SpinRow::builder()
            .title(t("windows-offset-y"))
            .adjustment(&offset_y_adj)
            .build();
        group.add(&offset_y_row);

        // Shadow colours support alpha
        let color_btn = color_dialog_button_with_alpha();
        let color_row = adw::ActionRow::builder().title(t("windows-color")).build();
        color_row.add_suffix(&color_btn);
        group.add(&color_row);

        let inactive_color_btn = color_dialog_button_with_alpha();
        let inactive_row = adw::ActionRow::builder()
            .title(t("windows-inactive-color"))
            .build();
        inactive_row.add_suffix(&inactive_color_btn);
        group.add(&inactive_row);

        // Wire switch toggle
        {
            let cb = output_cb.clone();
            let updating_ref = updating.clone();
            switch_row.connect_active_notify(move |row| {
                if updating_ref.get() {
                    return;
                }
                if let Some(ref callback) = *cb.borrow() {
                    callback(ShadowSectionOutput::Toggled(row.is_active()));
                }
            });
        }

        // Wire spin rows
        wire_spin_u32(&softness_row, &output_cb, &updating, |v| {
            ShadowSectionOutput::SoftnessChanged(v)
        });
        wire_spin_u32(&spread_row, &output_cb, &updating, |v| {
            ShadowSectionOutput::SpreadChanged(v)
        });
        wire_spin_i32(&offset_x_row, &output_cb, &updating, |v| {
            ShadowSectionOutput::OffsetXChanged(v)
        });
        wire_spin_i32(&offset_y_row, &output_cb, &updating, |v| {
            ShadowSectionOutput::OffsetYChanged(v)
        });

        // Wire colour buttons
        wire_color_button(&color_btn, &output_cb, &updating, |hex| {
            ShadowSectionOutput::ColorChanged(hex)
        });
        wire_color_button(&inactive_color_btn, &output_cb, &updating, |hex| {
            ShadowSectionOutput::InactiveColorChanged(hex)
        });

        Self {
            root: group,
            output_cb,
            switch_row,
            softness_row,
            spread_row,
            offset_x_row,
            offset_y_row,
            color_btn,
            inactive_color_btn,
            updating,
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub fn apply_props(
        &self,
        enabled: bool,
        softness: u32,
        spread: u32,
        offset_x: i32,
        offset_y: i32,
        color: &str,
        inactive_color: Option<&str>,
    ) {
        self.updating.set(true);

        self.switch_row.set_active(enabled);
        self.softness_row.set_value(f64::from(softness));
        self.spread_row.set_value(f64::from(spread));
        self.offset_x_row.set_value(f64::from(offset_x));
        self.offset_y_row.set_value(f64::from(offset_y));
        set_color_button(&self.color_btn, color);
        if let Some(inactive) = inactive_color {
            set_color_button(&self.inactive_color_btn, inactive);
        }

        self.updating.set(false);
    }

    pub fn connect_output<F: Fn(ShadowSectionOutput) + 'static>(&self, callback: F) {
        *self.output_cb.borrow_mut() = Some(Box::new(callback));
    }
}

fn color_dialog_button_with_alpha() -> gtk::ColorDialogButton {
    let dialog = gtk::ColorDialog::builder().with_alpha(true).build();
    gtk::ColorDialogButton::builder().dialog(&dialog).build()
}

fn parse_hex_color(hex: &str) -> gtk::gdk::RGBA {
    gtk::gdk::RGBA::parse(hex).unwrap_or(gtk::gdk::RGBA::BLACK)
}

fn set_color_button(btn: &gtk::ColorDialogButton, hex: &str) {
    btn.set_rgba(&parse_hex_color(hex));
}

fn rgba_to_hex_alpha(rgba: &gtk::gdk::RGBA) -> String {
    let a = (rgba.alpha() * 255.0) as u8;
    if a == 255 {
        format!(
            "#{:02x}{:02x}{:02x}",
            (rgba.red() * 255.0) as u8,
            (rgba.green() * 255.0) as u8,
            (rgba.blue() * 255.0) as u8,
        )
    } else {
        format!(
            "#{:02x}{:02x}{:02x}{:02x}",
            (rgba.red() * 255.0) as u8,
            (rgba.green() * 255.0) as u8,
            (rgba.blue() * 255.0) as u8,
            a,
        )
    }
}

fn wire_spin_u32<F>(
    row: &adw::SpinRow,
    output_cb: &OutputCallback,
    updating: &Rc<std::cell::Cell<bool>>,
    make_output: F,
) where
    F: Fn(u32) -> ShadowSectionOutput + 'static,
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

fn wire_spin_i32<F>(
    row: &adw::SpinRow,
    output_cb: &OutputCallback,
    updating: &Rc<std::cell::Cell<bool>>,
    make_output: F,
) where
    F: Fn(i32) -> ShadowSectionOutput + 'static,
{
    let cb = output_cb.clone();
    let updating_ref = updating.clone();
    row.connect_value_notify(move |row| {
        if updating_ref.get() {
            return;
        }
        if let Some(ref callback) = *cb.borrow() {
            callback(make_output(row.value() as i32));
        }
    });
}

fn wire_color_button<F>(
    btn: &gtk::ColorDialogButton,
    output_cb: &OutputCallback,
    updating: &Rc<std::cell::Cell<bool>>,
    make_output: F,
) where
    F: Fn(String) -> ShadowSectionOutput + 'static,
{
    let cb = output_cb.clone();
    let updating_ref = updating.clone();
    btn.connect_rgba_notify(move |btn| {
        if updating_ref.get() {
            return;
        }
        let hex = rgba_to_hex_alpha(&btn.rgba());
        if let Some(ref callback) = *cb.borrow() {
            callback(make_output(hex));
        }
    });
}
