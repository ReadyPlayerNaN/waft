//! Audio device sliders component.
//!
//! Subscribes to the `audio-device` entity type and renders sliders for
//! default output and input devices. Non-default devices are filtered out.
//! Output devices sort before input devices (weight 60 vs 65).

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use gtk::prelude::*;

use waft_protocol::entity;
use waft_protocol::entity::audio::AudioDeviceKind;
use waft_protocol::Urn;
use waft_ui_gtk::widgets::slider::{SliderProps, SliderWidget};

use crate::entity_store::{EntityActionCallback, EntityStore};

struct SliderEntry {
    widget: Rc<SliderWidget>,
    kind: AudioDeviceKind,
}

/// Renders volume sliders for default audio output and input devices.
///
/// Dynamically adds/removes sliders as devices become default or are removed.
/// Output devices are ordered before input devices.
pub struct AudioSlidersComponent {
    container: gtk::Box,
    sliders: Rc<RefCell<HashMap<String, SliderEntry>>>,
}

impl AudioSlidersComponent {
    pub fn new(store: &Rc<EntityStore>, action_callback: &EntityActionCallback) -> Self {
        let container = gtk::Box::new(gtk::Orientation::Vertical, 8);

        let sliders: Rc<RefCell<HashMap<String, SliderEntry>>> =
            Rc::new(RefCell::new(HashMap::new()));

        let store_ref = store.clone();
        let container_ref = container.clone();
        let sliders_ref = sliders.clone();
        let cb = action_callback.clone();

        store.subscribe_type(entity::audio::ENTITY_TYPE, move || {
            let entities: Vec<(Urn, entity::audio::AudioDevice)> =
                store_ref.get_entities_typed(entity::audio::ENTITY_TYPE);

            // Filter to default devices only
            let default_devices: Vec<(Urn, entity::audio::AudioDevice)> = entities
                .into_iter()
                .filter(|(_urn, device)| device.default)
                .collect();

            let mut sliders = sliders_ref.borrow_mut();

            // Collect URN strings of current default devices
            let current_urns: Vec<String> = default_devices
                .iter()
                .map(|(urn, _)| urn.as_str().to_string())
                .collect();

            // Remove sliders for devices no longer default/present
            let stale_keys: Vec<String> = sliders
                .keys()
                .filter(|k| !current_urns.contains(k))
                .cloned()
                .collect();

            for key in stale_keys {
                if let Some(entry) = sliders.remove(&key) {
                    container_ref.remove(&entry.widget.widget());
                }
            }

            // Update existing or create new sliders
            for (urn, device) in &default_devices {
                let urn_str = urn.as_str().to_string();
                let icon = slider_icon(device);

                if let Some(entry) = sliders.get(&urn_str) {
                    // Update existing slider in place
                    entry.widget.set_value(device.volume);
                    entry.widget.set_disabled(device.muted);
                    entry.widget.set_icon(&icon);
                } else {
                    // Create new slider
                    let slider = Rc::new(SliderWidget::new(
                        SliderProps {
                            icon,
                            value: device.volume,
                            disabled: device.muted,
                            expandable: false,
                            menu_id: None,
                        },
                        None,
                    ));

                    // Wire value_change -> set-volume action
                    let urn_for_value = urn.clone();
                    let cb_value = cb.clone();
                    slider.connect_value_change(move |v| {
                        cb_value(
                            urn_for_value.clone(),
                            "set-volume".to_string(),
                            serde_json::json!(v),
                        );
                    });

                    // Wire icon_click -> toggle-mute action
                    let urn_for_mute = urn.clone();
                    let cb_mute = cb.clone();
                    slider.connect_icon_click(move || {
                        cb_mute(
                            urn_for_mute.clone(),
                            "toggle-mute".to_string(),
                            serde_json::Value::Null,
                        );
                    });

                    sliders.insert(
                        urn_str,
                        SliderEntry {
                            widget: slider.clone(),
                            kind: device.kind,
                        },
                    );

                    // Append will be done after sorting below
                }
            }

            // Re-sort children: output (weight 60) before input (weight 65)
            // Remove all children, then re-append in sorted order
            while let Some(child) = container_ref.first_child() {
                container_ref.remove(&child);
            }

            let mut sorted: Vec<&SliderEntry> = sliders.values().collect();
            sorted.sort_by_key(|entry| match entry.kind {
                AudioDeviceKind::Output => 60,
                AudioDeviceKind::Input => 65,
            });

            for entry in sorted {
                container_ref.append(&entry.widget.widget());
            }
        });

        Self { container, sliders }
    }

    pub fn widget(&self) -> &gtk::Widget {
        self.container.upcast_ref()
    }
}

/// Select the appropriate icon for an audio device slider.
fn slider_icon(device: &entity::audio::AudioDevice) -> String {
    if device.muted {
        match device.kind {
            AudioDeviceKind::Output => "audio-volume-muted-symbolic".to_string(),
            AudioDeviceKind::Input => "microphone-sensitivity-muted-symbolic".to_string(),
        }
    } else {
        device.icon.clone()
    }
}
