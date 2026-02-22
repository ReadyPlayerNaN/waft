//! Audio device row widget for device selection menus.
//!
//! A horizontal button row showing device type icon, optional connection icon
//! (e.g. bluetooth), device name, and a checkmark when this device is the
//! active/default device.

use std::cell::RefCell;
use std::rc::Rc;

use gtk::prelude::*;
use waft_core::Callback;
use waft_protocol::entity::audio::AudioDeviceKind;

use crate::audio::icon::{audio_connection_icon, audio_device_icon};
use crate::icons::IconWidget;

/// Properties for initializing an audio device row.
pub struct AudioDeviceRowProps {
    pub device_type: String,
    pub connection_type: Option<String>,
    pub kind: AudioDeviceKind,
    pub name: String,
    pub active: bool,
}

/// Output events from the audio device row.
pub enum AudioDeviceRowOutput {
    SelectAsDefault,
}

/// A horizontal button row for a single audio device.
///
/// Layout: `Button > Box(H) > [icon_box(device_icon + connection_icon), name_label(hexpand), right_box(checkmark)]`
pub struct AudioDeviceRow {
    pub root: gtk::Button,
    name_label: gtk::Label,
    device_icon: IconWidget,
    connection_icon: IconWidget,
    connection_icon_widget: gtk::Widget,
    checkmark_widget: gtk::Widget,
    on_output: Callback<AudioDeviceRowOutput>,
}

impl AudioDeviceRow {
    pub fn new(props: AudioDeviceRowProps) -> Self {
        let inner = gtk::Box::builder()
            .orientation(gtk::Orientation::Horizontal)
            .spacing(8)
            .build();

        // Left box: device type icon + connection icon
        let icon_box = gtk::Box::builder()
            .orientation(gtk::Orientation::Horizontal)
            .spacing(4)
            .valign(gtk::Align::Center)
            .build();

        let device_icon_name = audio_device_icon(&props.device_type, props.kind);
        let device_icon = IconWidget::from_name(device_icon_name, 16);
        icon_box.append(device_icon.widget());

        let conn_icon_name = props
            .connection_type
            .as_deref()
            .and_then(audio_connection_icon);
        let connection_icon =
            IconWidget::from_name(conn_icon_name.unwrap_or("bluetooth-symbolic"), 16);
        let connection_icon_widget = connection_icon.widget().clone().upcast::<gtk::Widget>();
        connection_icon_widget.set_visible(conn_icon_name.is_some());
        icon_box.append(&connection_icon_widget);

        inner.append(&icon_box);

        // Center: device name (expands to fill)
        let name_label = gtk::Label::builder()
            .label(&props.name)
            .hexpand(true)
            .xalign(0.0)
            .ellipsize(gtk::pango::EllipsizeMode::End)
            .build();
        inner.append(&name_label);

        // Right box: checkmark (visible only when active)
        let right_box = gtk::Box::builder()
            .orientation(gtk::Orientation::Horizontal)
            .spacing(4)
            .valign(gtk::Align::Center)
            .build();

        let checkmark = IconWidget::from_name("object-select-symbolic", 16);
        let checkmark_widget = checkmark.widget().clone().upcast::<gtk::Widget>();
        checkmark_widget.set_visible(props.active);
        right_box.append(&checkmark_widget);

        inner.append(&right_box);

        let button = gtk::Button::builder()
            .child(&inner)
            .css_classes(["flat", "device-row"])
            .build();

        let on_output: Callback<AudioDeviceRowOutput> = Rc::new(RefCell::new(None));
        let on_output_ref = on_output.clone();
        button.connect_clicked(move |_| {
            if let Some(ref callback) = *on_output_ref.borrow() {
                callback(AudioDeviceRowOutput::SelectAsDefault);
            }
        });

        Self {
            root: button,
            name_label,
            device_icon,
            connection_icon,
            connection_icon_widget,
            checkmark_widget,
            on_output,
        }
    }

    pub fn connect_output<F>(&self, callback: F)
    where
        F: Fn(AudioDeviceRowOutput) + 'static,
    {
        *self.on_output.borrow_mut() = Some(Box::new(callback));
    }

    pub fn set_name(&self, name: &str) {
        self.name_label.set_label(name);
    }

    pub fn set_device_icon(&self, device_type: &str, kind: AudioDeviceKind) {
        self.device_icon.set_icon(audio_device_icon(device_type, kind));
    }

    pub fn set_connection_icon(&self, connection_type: Option<&str>) {
        let icon_name = connection_type.and_then(audio_connection_icon);
        if let Some(name) = icon_name {
            self.connection_icon.set_icon(name);
            self.connection_icon_widget.set_visible(true);
        } else {
            self.connection_icon_widget.set_visible(false);
        }
    }

    pub fn set_active(&self, active: bool) {
        self.checkmark_widget.set_visible(active);
    }
}
