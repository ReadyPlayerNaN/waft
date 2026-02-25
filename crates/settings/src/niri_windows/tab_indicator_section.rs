//! Tab indicator settings section -- dumb widget.
//!
//! Controls for niri tab indicator: enabled toggle, position combo, gap, width,
//! corner radius, and three colour pickers.

use std::cell::RefCell;
use std::rc::Rc;

use adw::prelude::*;

use crate::i18n::t;

/// Output events from the tab indicator section.
pub enum TabIndicatorSectionOutput {
    Toggled(bool),
    PositionChanged(String),
    GapChanged(u32),
    WidthChanged(u32),
    CornerRadiusChanged(u32),
    ActiveColorChanged(String),
    InactiveColorChanged(String),
    UrgentColorChanged(String),
}

type OutputCallback = Rc<RefCell<Option<Box<dyn Fn(TabIndicatorSectionOutput)>>>>;

/// Position options in combo row order.
const POSITION_OPTIONS: &[&str] = &["left", "right", "top", "bottom"];

/// Tab indicator controls.
pub struct TabIndicatorSection {
    pub root: adw::PreferencesGroup,
    output_cb: OutputCallback,
    switch_row: adw::SwitchRow,
    position_row: adw::ComboRow,
    gap_row: adw::SpinRow,
    width_row: adw::SpinRow,
    corner_radius_row: adw::SpinRow,
    active_color_btn: gtk::ColorDialogButton,
    inactive_color_btn: gtk::ColorDialogButton,
    urgent_color_btn: gtk::ColorDialogButton,
    updating: Rc<std::cell::Cell<bool>>,
}

impl TabIndicatorSection {
    pub fn new() -> Self {
        let output_cb: OutputCallback = Rc::new(RefCell::new(None));
        let updating = Rc::new(std::cell::Cell::new(false));

        let group = adw::PreferencesGroup::builder()
            .title(t("windows-tab-indicator"))
            .build();

        let switch_row = adw::SwitchRow::builder()
            .title(t("windows-enabled"))
            .build();
        group.add(&switch_row);

        // Position combo
        let position_model = gtk::StringList::new(&[
            &t("windows-position-left"),
            &t("windows-position-right"),
            &t("windows-position-top"),
            &t("windows-position-bottom"),
        ]);
        let position_row = adw::ComboRow::builder()
            .title(t("windows-position"))
            .model(&position_model)
            .selected(0)
            .build();
        group.add(&position_row);

        let gap_adj = gtk::Adjustment::new(4.0, 0.0, 64.0, 1.0, 4.0, 0.0);
        let gap_row = adw::SpinRow::builder()
            .title(t("windows-gap"))
            .adjustment(&gap_adj)
            .build();
        group.add(&gap_row);

        let width_adj = gtk::Adjustment::new(4.0, 0.0, 32.0, 1.0, 4.0, 0.0);
        let width_row = adw::SpinRow::builder()
            .title(t("windows-width"))
            .adjustment(&width_adj)
            .build();
        group.add(&width_row);

        let corner_radius_adj = gtk::Adjustment::new(8.0, 0.0, 32.0, 1.0, 4.0, 0.0);
        let corner_radius_row = adw::SpinRow::builder()
            .title(t("windows-corner-radius"))
            .adjustment(&corner_radius_adj)
            .build();
        group.add(&corner_radius_row);

        let active_color_btn = color_dialog_button();
        let active_row = adw::ActionRow::builder()
            .title(t("windows-active-color"))
            .build();
        active_row.add_suffix(&active_color_btn);
        group.add(&active_row);

        let inactive_color_btn = color_dialog_button();
        let inactive_row = adw::ActionRow::builder()
            .title(t("windows-inactive-color"))
            .build();
        inactive_row.add_suffix(&inactive_color_btn);
        group.add(&inactive_row);

        let urgent_color_btn = color_dialog_button();
        let urgent_row = adw::ActionRow::builder()
            .title(t("windows-urgent-color"))
            .build();
        urgent_row.add_suffix(&urgent_color_btn);
        group.add(&urgent_row);

        // Wire switch
        {
            let cb = output_cb.clone();
            let updating_ref = updating.clone();
            switch_row.connect_active_notify(move |row| {
                if updating_ref.get() {
                    return;
                }
                if let Some(ref callback) = *cb.borrow() {
                    callback(TabIndicatorSectionOutput::Toggled(row.is_active()));
                }
            });
        }

        // Wire position combo
        {
            let cb = output_cb.clone();
            let updating_ref = updating.clone();
            position_row.connect_selected_notify(move |row| {
                if updating_ref.get() {
                    return;
                }
                let selected = row.selected() as usize;
                let position = POSITION_OPTIONS
                    .get(selected)
                    .unwrap_or(&"left")
                    .to_string();
                if let Some(ref callback) = *cb.borrow() {
                    callback(TabIndicatorSectionOutput::PositionChanged(position));
                }
            });
        }

        // Wire spin rows
        wire_spin_u32(&gap_row, &output_cb, &updating, |v| {
            TabIndicatorSectionOutput::GapChanged(v)
        });
        wire_spin_u32(&width_row, &output_cb, &updating, |v| {
            TabIndicatorSectionOutput::WidthChanged(v)
        });
        wire_spin_u32(&corner_radius_row, &output_cb, &updating, |v| {
            TabIndicatorSectionOutput::CornerRadiusChanged(v)
        });

        // Wire colour buttons
        wire_color_button(&active_color_btn, &output_cb, &updating, |hex| {
            TabIndicatorSectionOutput::ActiveColorChanged(hex)
        });
        wire_color_button(&inactive_color_btn, &output_cb, &updating, |hex| {
            TabIndicatorSectionOutput::InactiveColorChanged(hex)
        });
        wire_color_button(&urgent_color_btn, &output_cb, &updating, |hex| {
            TabIndicatorSectionOutput::UrgentColorChanged(hex)
        });

        Self {
            root: group,
            output_cb,
            switch_row,
            position_row,
            gap_row,
            width_row,
            corner_radius_row,
            active_color_btn,
            inactive_color_btn,
            urgent_color_btn,
            updating,
        }
    }

    pub fn apply_props(
        &self,
        enabled: bool,
        position: &str,
        gap: u32,
        width: u32,
        corner_radius: u32,
        active_color: &str,
        inactive_color: &str,
        urgent_color: Option<&str>,
    ) {
        self.updating.set(true);

        self.switch_row.set_active(enabled);

        let pos_idx = POSITION_OPTIONS
            .iter()
            .position(|p| *p == position)
            .unwrap_or(0);
        self.position_row.set_selected(pos_idx as u32);

        self.gap_row.set_value(f64::from(gap));
        self.width_row.set_value(f64::from(width));
        self.corner_radius_row.set_value(f64::from(corner_radius));
        set_color_button(&self.active_color_btn, active_color);
        set_color_button(&self.inactive_color_btn, inactive_color);
        if let Some(color) = urgent_color {
            set_color_button(&self.urgent_color_btn, color);
        }

        self.updating.set(false);
    }

    pub fn set_colors_sensitive(&self, sensitive: bool) {
        self.active_color_btn.set_sensitive(sensitive);
        self.inactive_color_btn.set_sensitive(sensitive);
        self.urgent_color_btn.set_sensitive(sensitive);
    }

    pub fn connect_output<F: Fn(TabIndicatorSectionOutput) + 'static>(&self, callback: F) {
        *self.output_cb.borrow_mut() = Some(Box::new(callback));
    }
}

fn color_dialog_button() -> gtk::ColorDialogButton {
    let dialog = gtk::ColorDialog::builder().with_alpha(false).build();
    gtk::ColorDialogButton::builder().dialog(&dialog).build()
}

fn parse_hex_color(hex: &str) -> gtk::gdk::RGBA {
    gtk::gdk::RGBA::parse(hex).unwrap_or(gtk::gdk::RGBA::BLACK)
}

fn set_color_button(btn: &gtk::ColorDialogButton, hex: &str) {
    btn.set_rgba(&parse_hex_color(hex));
}

fn rgba_to_hex(rgba: &gtk::gdk::RGBA) -> String {
    format!(
        "#{:02x}{:02x}{:02x}",
        (rgba.red() * 255.0) as u8,
        (rgba.green() * 255.0) as u8,
        (rgba.blue() * 255.0) as u8,
    )
}

fn wire_spin_u32<F>(
    row: &adw::SpinRow,
    output_cb: &OutputCallback,
    updating: &Rc<std::cell::Cell<bool>>,
    make_output: F,
) where
    F: Fn(u32) -> TabIndicatorSectionOutput + 'static,
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

fn wire_color_button<F>(
    btn: &gtk::ColorDialogButton,
    output_cb: &OutputCallback,
    updating: &Rc<std::cell::Cell<bool>>,
    make_output: F,
) where
    F: Fn(String) -> TabIndicatorSectionOutput + 'static,
{
    let cb = output_cb.clone();
    let updating_ref = updating.clone();
    btn.connect_rgba_notify(move |btn| {
        if updating_ref.get() {
            return;
        }
        let hex = rgba_to_hex(&btn.rgba());
        if let Some(ref callback) = *cb.borrow() {
            callback(make_output(hex));
        }
    });
}
