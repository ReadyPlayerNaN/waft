//! Backup method row widget for menu method lists.
//!
//! A horizontal button row showing a method icon, method name, and
//! a switch indicator for enabled state.

use std::cell::RefCell;
use std::rc::Rc;

use gtk::prelude::*;
use waft_core::Callback;

use crate::icons::IconWidget;

/// Properties for initializing a backup method row.
pub struct BackupMethodRowProps {
    pub icon: String,
    pub name: String,
    pub enabled: bool,
}

/// Output events from the backup method row.
pub enum BackupMethodRowOutput {
    ToggleMethod,
}

/// A horizontal button row for a single backup method.
///
/// Layout: `Button > Box(H) > [icon, name_label(hexpand), switch]`
pub struct BackupMethodRow {
    pub root: gtk::Button,
    name_label: gtk::Label,
    icon: IconWidget,
    switch: gtk::Switch,
    on_output: Callback<BackupMethodRowOutput>,
}

impl BackupMethodRow {
    pub fn new(props: BackupMethodRowProps) -> Self {
        let inner = gtk::Box::builder()
            .orientation(gtk::Orientation::Horizontal)
            .spacing(8)
            .build();

        // Left: method icon
        let icon = IconWidget::from_name(&props.icon, 16);
        inner.append(icon.widget());

        // Center: method name (expands to fill)
        let name_label = gtk::Label::builder()
            .label(&props.name)
            .hexpand(true)
            .xalign(0.0)
            .ellipsize(gtk::pango::EllipsizeMode::End)
            .build();
        inner.append(&name_label);

        // Right: enabled switch
        let switch = gtk::Switch::builder()
            .active(props.enabled)
            .sensitive(false) // display-only
            .valign(gtk::Align::Center)
            .css_classes(["device-switch"])
            .build();
        inner.append(&switch);

        let button = gtk::Button::builder()
            .child(&inner)
            .css_classes(["flat", "device-row"])
            .build();

        let on_output: Callback<BackupMethodRowOutput> = Rc::new(RefCell::new(None));
        let on_output_ref = on_output.clone();
        button.connect_clicked(move |_| {
            if let Some(ref callback) = *on_output_ref.borrow() {
                callback(BackupMethodRowOutput::ToggleMethod);
            }
        });

        Self {
            root: button,
            name_label,
            icon,
            switch,
            on_output,
        }
    }

    pub fn connect_output<F>(&self, callback: F)
    where
        F: Fn(BackupMethodRowOutput) + 'static,
    {
        *self.on_output.borrow_mut() = Some(Box::new(callback));
    }

    pub fn set_name(&self, name: &str) {
        self.name_label.set_label(name);
    }

    pub fn set_icon(&self, icon_name: &str) {
        self.icon.set_icon(icon_name);
    }

    pub fn set_enabled(&self, enabled: bool) {
        self.switch.set_active(enabled);
    }
}
