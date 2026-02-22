//! Connection row widget for toggleable connection lists.
//!
//! A horizontal button row showing a connection name and a spinner/switch
//! indicator for connection state. Reusable for VPN, ethernet profiles, etc.

use std::cell::RefCell;
use std::rc::Rc;

use gtk::prelude::*;
use waft_core::Callback;

use crate::icons::IconWidget;

/// Properties for initializing a connection row.
pub struct ConnectionRowProps {
    pub name: String,
    pub active: bool,
    pub transitioning: bool,
    /// Optional leading icon name (e.g. "network-vpn-symbolic").
    pub icon: Option<String>,
}

/// Output events from the connection row.
pub enum ConnectionRowOutput {
    Toggle,
}

/// A horizontal button row for a single toggleable connection.
///
/// Layout: `Button > Box(H) > [icon_box(icon 16px), name_label(hexpand), right_box(spinner + switch)]`
pub struct ConnectionRow {
    pub root: gtk::Button,
    icon_box: gtk::Box,
    icon_widget: Option<IconWidget>,
    name_label: gtk::Label,
    spinner: gtk::Spinner,
    switch: gtk::Switch,
    on_output: Callback<ConnectionRowOutput>,
}

impl ConnectionRow {
    pub fn new(props: ConnectionRowProps) -> Self {
        let inner = gtk::Box::builder()
            .orientation(gtk::Orientation::Horizontal)
            .spacing(8)
            .build();

        // Left box: connection icon
        let icon_box = gtk::Box::builder()
            .orientation(gtk::Orientation::Horizontal)
            .spacing(4)
            .valign(gtk::Align::Center)
            .build();

        // Always append icon_box (may be empty)
        inner.prepend(&icon_box);

        // Populate it if we have an initial icon
        let icon_widget = props.icon.as_ref().map(|icon_name| {
            let icon = IconWidget::from_name(icon_name, 16);
            icon_box.append(icon.widget());
            icon
        });

        // Connection name (expands to fill)
        let name_label = gtk::Label::builder()
            .label(&props.name)
            .hexpand(true)
            .xalign(0.0)
            .ellipsize(gtk::pango::EllipsizeMode::End)
            .build();
        inner.append(&name_label);

        // Right box: spinner (hidden by default) + connection switch
        let right_box = gtk::Box::builder()
            .orientation(gtk::Orientation::Horizontal)
            .spacing(4)
            .valign(gtk::Align::Center)
            .build();

        let spinner = gtk::Spinner::builder()
            .visible(props.transitioning)
            .spinning(props.transitioning)
            .build();
        right_box.append(&spinner);

        let switch = gtk::Switch::builder()
            .active(props.active)
            .sensitive(false) // display-only
            .valign(gtk::Align::Center)
            .css_classes(["device-switch"])
            .build();
        right_box.append(&switch);

        inner.append(&right_box);

        let button = gtk::Button::builder()
            .child(&inner)
            .css_classes(["flat", "device-row"])
            .sensitive(!props.transitioning)
            .build();

        let on_output: Callback<ConnectionRowOutput> = Rc::new(RefCell::new(None));
        let on_output_ref = on_output.clone();
        button.connect_clicked(move |_| {
            if let Some(ref callback) = *on_output_ref.borrow() {
                callback(ConnectionRowOutput::Toggle);
            }
        });

        Self {
            root: button,
            icon_box,
            icon_widget,
            name_label,
            spinner,
            switch,
            on_output,
        }
    }

    pub fn connect_output<F>(&self, callback: F)
    where
        F: Fn(ConnectionRowOutput) + 'static,
    {
        *self.on_output.borrow_mut() = Some(Box::new(callback));
    }

    pub fn set_name(&self, name: &str) {
        self.name_label.set_label(name);
    }

    pub fn set_active(&self, active: bool) {
        self.switch.set_active(active);
    }

    pub fn set_transitioning(&self, transitioning: bool) {
        self.spinner.set_visible(transitioning);
        self.spinner.set_spinning(transitioning);
        self.root.set_sensitive(!transitioning);
    }

    pub fn set_icon(&mut self, icon_name: Option<&str>) {
        // Remove existing icon widget if any
        if let Some(ref old_icon) = self.icon_widget {
            self.icon_box.remove(old_icon.widget());
        }
        // Add new icon if provided
        self.icon_widget = icon_name.map(|name| {
            let icon = IconWidget::from_name(name, 16);
            self.icon_box.append(icon.widget());
            icon
        });
    }
}
