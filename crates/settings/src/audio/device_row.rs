//! Per-device audio row widget.
//!
//! Dumb widget displaying a single audio device as an `adw::ActionRow`
//! with device icon, name, volume slider, mute toggle, and default indicator.

use std::cell::RefCell;
use std::rc::Rc;

use adw::prelude::*;
use waft_ui_gtk::widgets::icon::IconWidget;

use crate::i18n::t;

/// Props for creating or updating an audio device row.
pub struct AudioDeviceRowProps {
    pub name: String,
    pub icon: String,
    pub connection_icon: Option<String>,
    pub volume: f64,
    pub muted: bool,
    pub default: bool,
}

/// Output events from an audio device row.
pub enum AudioDeviceRowOutput {
    /// Volume slider changed (0.0 - 1.0).
    SetVolume(f64),
    /// Mute button clicked.
    ToggleMute,
    /// Set this device as the default.
    SetDefault,
}

/// Callback type for device row output events.
type OutputCallback = Rc<RefCell<Option<Box<dyn Fn(AudioDeviceRowOutput)>>>>;

/// A single audio device row in the settings page.
pub struct AudioDeviceRow {
    pub root: adw::ActionRow,
    icon: IconWidget,
    connection_icon_widget: IconWidget,
    slider: gtk::Scale,
    mute_button: gtk::Button,
    default_button: gtk::Button,
    output_cb: OutputCallback,
    /// Suppresses change signals while applying props programmatically.
    updating: Rc<RefCell<bool>>,
}

impl AudioDeviceRow {
    pub fn new(props: &AudioDeviceRowProps) -> Self {
        let icon = IconWidget::from_name(&props.icon, 16);
        let connection_icon_widget = IconWidget::from_name(
            props.connection_icon.as_deref().unwrap_or(""),
            16,
        );
        connection_icon_widget
            .widget()
            .set_visible(props.connection_icon.is_some());

        let row = adw::ActionRow::builder()
            .title(&props.name)
            .activatable(false)
            .build();

        row.add_prefix(icon.widget());
        row.add_prefix(connection_icon_widget.widget());

        // Volume slider
        let slider = gtk::Scale::builder()
            .orientation(gtk::Orientation::Horizontal)
            .hexpand(true)
            .width_request(150)
            .valign(gtk::Align::Center)
            .build();
        slider.set_range(0.0, 1.0);
        slider.set_value(props.volume);
        slider.set_draw_value(false);

        // Mute toggle button
        let mute_button = gtk::Button::builder()
            .valign(gtk::Align::Center)
            .css_classes(["flat"])
            .build();

        // Set default button
        let default_button = gtk::Button::builder()
            .label(t("audio-set-default"))
            .valign(gtk::Align::Center)
            .css_classes(["flat"])
            .build();

        row.add_suffix(&default_button);
        row.add_suffix(&mute_button);
        row.add_suffix(&slider);

        let output_cb: OutputCallback = Rc::new(RefCell::new(None));
        let updating = Rc::new(RefCell::new(false));

        // Wire slider value-changed
        {
            let cb = output_cb.clone();
            let updating_ref = updating.clone();
            slider.connect_value_changed(move |scale| {
                if *updating_ref.borrow() {
                    return;
                }
                if let Some(ref callback) = *cb.borrow() {
                    callback(AudioDeviceRowOutput::SetVolume(scale.value()));
                }
            });
        }

        // Wire mute button
        {
            let cb = output_cb.clone();
            mute_button.connect_clicked(move |_| {
                if let Some(ref callback) = *cb.borrow() {
                    callback(AudioDeviceRowOutput::ToggleMute);
                }
            });
        }

        // Wire default button
        {
            let cb = output_cb.clone();
            default_button.connect_clicked(move |_| {
                if let Some(ref callback) = *cb.borrow() {
                    callback(AudioDeviceRowOutput::SetDefault);
                }
            });
        }

        let device_row = Self {
            root: row,
            icon,
            connection_icon_widget,
            slider,
            mute_button,
            default_button,
            output_cb,
            updating,
        };

        device_row.apply_props(props);
        device_row
    }

    /// Update the row to reflect new device state.
    pub fn apply_props(&self, props: &AudioDeviceRowProps) {
        *self.updating.borrow_mut() = true;

        self.root.set_title(&props.name);
        self.icon.set_icon(&props.icon);

        if let Some(ref conn_icon) = props.connection_icon {
            self.connection_icon_widget.set_icon(conn_icon);
            self.connection_icon_widget.widget().set_visible(true);
        } else {
            self.connection_icon_widget.widget().set_visible(false);
        }

        self.slider.set_value(props.volume);

        // Mute button icon
        let mute_icon = if props.muted {
            "audio-volume-muted-symbolic"
        } else {
            "audio-volume-high-symbolic"
        };
        self.mute_button.set_icon_name(mute_icon);

        // Default indicator
        if props.default {
            self.root.set_subtitle(&t("audio-default-device"));
            self.default_button.set_visible(false);
        } else {
            self.root.set_subtitle("");
            self.default_button.set_visible(true);
        }

        *self.updating.borrow_mut() = false;
    }

    /// Register a callback for device row output events.
    pub fn connect_output<F: Fn(AudioDeviceRowOutput) + 'static>(&self, callback: F) {
        *self.output_cb.borrow_mut() = Some(Box::new(callback));
    }
}
