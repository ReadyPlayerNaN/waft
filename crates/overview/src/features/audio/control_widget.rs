//! Audio control widget.
//!
//! A thin wrapper around SliderControlWidget that adds audio-specific behavior
//! (muted state, device menu, icon switching).

use std::cell::RefCell;
use std::rc::Rc;

use gtk::prelude::*;

use super::device_menu::{AudioDeviceDisplay, AudioDeviceMenuOutput, AudioDeviceMenuWidget};
use super::store::AudioDevice;
use crate::common::Callback;
use crate::menu_state::MenuStore;
use crate::ui::icon::resolve_themed_icon;
use crate::ui::slider_control::{SliderControlOutput, SliderControlWidget};

/// Resolve a muted variant of the given icon name with theme fallbacks.
///
/// Tries `"{stem}-muted-symbolic"` first (works for microphone icons),
/// then strips the last segment and tries `"{prefix}-muted-symbolic"`
/// (turns `audio-volume-high` into `audio-volume-muted`).
/// Falls back to the original icon if nothing is found.
fn muted_icon_name(base_icon: &str) -> String {
    let stem = base_icon.trim_end_matches("-symbolic");

    // Try "{stem}-muted-symbolic" first (e.g. audio-input-microphone-muted-symbolic)
    let candidate = format!("{}-muted-symbolic", stem);
    if let Some(resolved) = resolve_themed_icon(&candidate) {
        return resolved;
    }

    // Try stripping the last segment: "audio-volume-high" → "audio-volume-muted-symbolic"
    if let Some((prefix, _)) = stem.rsplit_once('-') {
        let candidate = format!("{}-muted-symbolic", prefix);
        if let Some(resolved) = resolve_themed_icon(&candidate) {
            return resolved;
        }
    }

    // Nothing found — return the base icon unchanged
    base_icon.to_string()
}

/// Output events from the audio control widget.
#[derive(Debug, Clone)]
pub enum AudioControlOutput {
    VolumeChanged(f64),
    ToggleMute,
    SelectDevice(String),
}

/// Properties for initializing an audio control widget.
#[derive(Debug, Clone)]
pub struct AudioControlProps {
    pub icon: String,
    pub volume: f64,
    pub muted: bool,
    pub devices: Vec<AudioDevice>,
    pub default_device: Option<String>,
}

/// Combined audio control widget with slider and expandable device menu.
pub struct AudioControlWidget {
    pub root: gtk::Box,
    slider: SliderControlWidget,
    device_menu: AudioDeviceMenuWidget,
    muted: Rc<RefCell<bool>>,
    icon_name: Rc<RefCell<String>>,
    on_output: Callback<AudioControlOutput>,
}

impl AudioControlWidget {
    /// Create a new audio control widget.
    pub fn new(props: AudioControlProps, menu_store: Rc<MenuStore>) -> Self {
        // Create device menu and set initial devices
        let device_menu = AudioDeviceMenuWidget::new();
        let devices: Vec<AudioDeviceDisplay> = props
            .devices
            .iter()
            .map(|d| AudioDeviceDisplay::from((d, props.default_device.as_ref() == Some(&d.id))))
            .collect();
        device_menu.set_devices(devices);

        // Create slider with device menu as expandable content
        let slider = SliderControlWidget::new(
            &props.icon,
            props.volume,
            Some(&device_menu.root),
            menu_store,
        );

        let muted = Rc::new(RefCell::new(props.muted));
        let icon_name = Rc::new(RefCell::new(props.icon.clone()));
        let on_output: Callback<AudioControlOutput> = Rc::new(RefCell::new(None));

        // Apply initial muted state
        if props.muted {
            slider.slider_row().add_css_class("muted");
            slider.set_icon(&muted_icon_name(&props.icon));
        }

        // Connect slider outputs
        let on_output_ref = on_output.clone();
        slider.connect_output(move |event| match event {
            SliderControlOutput::ValueChanged(value) => {
                if let Some(ref callback) = *on_output_ref.borrow() {
                    callback(AudioControlOutput::VolumeChanged(value));
                }
            }
            SliderControlOutput::IconClicked => {
                if let Some(ref callback) = *on_output_ref.borrow() {
                    callback(AudioControlOutput::ToggleMute);
                }
            }
        });

        // Connect device menu outputs
        let on_output_ref = on_output.clone();
        device_menu.connect_output(move |event| match event {
            AudioDeviceMenuOutput::SelectDevice(device_id) => {
                if let Some(ref callback) = *on_output_ref.borrow() {
                    callback(AudioControlOutput::SelectDevice(device_id));
                }
            }
        });

        let root = slider.root.clone();

        Self {
            root,
            slider,
            device_menu,
            muted,
            icon_name,
            on_output,
        }
    }

    /// Set the callback for output events.
    pub fn connect_output<F>(&self, callback: F)
    where
        F: Fn(AudioControlOutput) + 'static,
    {
        *self.on_output.borrow_mut() = Some(Box::new(callback));
    }

    /// Update the volume (0.0 - 1.0).
    pub fn set_volume(&self, volume: f64) {
        self.slider.set_value(volume);
    }

    /// Update the muted state.
    pub fn set_muted(&self, muted: bool) {
        *self.muted.borrow_mut() = muted;
        let base_icon = self.icon_name.borrow().clone();

        if muted {
            self.slider.slider_row().add_css_class("muted");
            self.slider.set_icon(&muted_icon_name(&base_icon));
        } else {
            self.slider.slider_row().remove_css_class("muted");
            self.slider.set_icon(&base_icon);
        }
    }

    /// Update the device list.
    pub fn set_devices(&self, devices: Vec<AudioDevice>, default_device: Option<&str>) {
        let display_devices: Vec<AudioDeviceDisplay> = devices
            .iter()
            .map(|d| AudioDeviceDisplay::from((d, default_device == Some(&d.id))))
            .collect();
        self.device_menu.set_devices(display_devices);
    }
}
