//! Border settings section -- dumb widget.
//!
//! Same shape as focus ring: enabled toggle, width, and three colour pickers.

use std::cell::RefCell;
use std::rc::Rc;

use adw::prelude::*;

use crate::i18n::t;

/// Output events from the border section.
pub enum BorderSectionOutput {
    Toggled(bool),
    WidthChanged(u32),
    ActiveColorChanged(String),
    InactiveColorChanged(String),
    UrgentColorChanged(String),
}

type OutputCallback = Rc<RefCell<Option<Box<dyn Fn(BorderSectionOutput)>>>>;

/// Border controls.
pub struct BorderSection {
    pub root: adw::PreferencesGroup,
    output_cb: OutputCallback,
    switch_row: adw::SwitchRow,
    width_row: adw::SpinRow,
    active_color_btn: gtk::ColorDialogButton,
    inactive_color_btn: gtk::ColorDialogButton,
    urgent_color_btn: gtk::ColorDialogButton,
    updating: Rc<std::cell::Cell<bool>>,
}

impl BorderSection {
    pub fn new() -> Self {
        let output_cb: OutputCallback = Rc::new(RefCell::new(None));
        let updating = Rc::new(std::cell::Cell::new(false));

        let group = adw::PreferencesGroup::builder()
            .title(t("windows-border"))
            .build();

        let switch_row = adw::SwitchRow::builder()
            .title(t("windows-enabled"))
            .build();
        group.add(&switch_row);

        let width_adj = gtk::Adjustment::new(4.0, 0.0, 32.0, 1.0, 4.0, 0.0);
        let width_row = adw::SpinRow::builder()
            .title(t("windows-width"))
            .adjustment(&width_adj)
            .build();
        group.add(&width_row);

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

        // Wire switch toggle
        {
            let cb = output_cb.clone();
            let updating_ref = updating.clone();
            switch_row.connect_active_notify(move |row| {
                if updating_ref.get() {
                    return;
                }
                if let Some(ref callback) = *cb.borrow() {
                    callback(BorderSectionOutput::Toggled(row.is_active()));
                }
            });
        }

        // Wire width
        {
            let cb = output_cb.clone();
            let updating_ref = updating.clone();
            width_row.connect_value_notify(move |row| {
                if updating_ref.get() {
                    return;
                }
                if let Some(ref callback) = *cb.borrow() {
                    callback(BorderSectionOutput::WidthChanged(row.value() as u32));
                }
            });
        }

        wire_color_button(&active_color_btn, &output_cb, &updating, |hex| {
            BorderSectionOutput::ActiveColorChanged(hex)
        });
        wire_color_button(&inactive_color_btn, &output_cb, &updating, |hex| {
            BorderSectionOutput::InactiveColorChanged(hex)
        });
        wire_color_button(&urgent_color_btn, &output_cb, &updating, |hex| {
            BorderSectionOutput::UrgentColorChanged(hex)
        });

        Self {
            root: group,
            output_cb,
            switch_row,
            width_row,
            active_color_btn,
            inactive_color_btn,
            urgent_color_btn,
            updating,
        }
    }

    pub fn apply_props(
        &self,
        enabled: bool,
        width: u32,
        active_color: &str,
        inactive_color: &str,
        urgent_color: Option<&str>,
    ) {
        self.updating.set(true);

        self.switch_row.set_active(enabled);
        self.width_row.set_value(f64::from(width));
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

    pub fn connect_output<F: Fn(BorderSectionOutput) + 'static>(&self, callback: F) {
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

fn wire_color_button<F>(
    btn: &gtk::ColorDialogButton,
    output_cb: &OutputCallback,
    updating: &Rc<std::cell::Cell<bool>>,
    make_output: F,
) where
    F: Fn(String) -> BorderSectionOutput + 'static,
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
