//! Audio device sliders component.
//!
//! Subscribes to the `audio-device` entity type and renders expandable sliders
//! for default output and input devices. Each slider has a menu listing all
//! devices of that kind, allowing the user to switch the default device.
//! Output devices sort before input devices (weight 60 vs 65).

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use std::time::Duration;

use gtk::prelude::*;
use waft_protocol::Urn;
use waft_protocol::entity;
use waft_protocol::entity::audio::AudioDeviceKind;
use waft_ui_gtk::audio::device_row::AudioDeviceRowProps;
use waft_ui_gtk::audio::slider_menu::{
    AudioSliderDevice, AudioSliderMenu, AudioSliderMenuOutput, AudioSliderMenuProps,
};
use waft_ui_gtk::menu_state::{is_menu_open, menu_id_for_widget, toggle_menu};
use waft_ui_gtk::vdom::Component;

use super::throttled_sender::ThrottledSender;
use waft_client::{EntityActionCallback, EntityStore};

/// A slider entry keyed by device kind (output or input).
struct SliderEntry {
    menu: Rc<AudioSliderMenu>,
    kind: AudioDeviceKind,
    throttle: ThrottledSender,
    /// Current default device URN (updated on each reconciliation).
    /// Shared with the output callback so it always uses the latest URN.
    current_urn: Rc<RefCell<Option<Urn>>>,
    /// Latest rendered props — updated by both entity and menu-store subscriptions
    /// so each subscription can patch only its own fields before re-rendering.
    current_props: Rc<RefCell<AudioSliderMenuProps>>,
}

/// Renders volume sliders for default audio output and input devices with
/// expandable device selection menus.
///
/// Each slider controls the default device of its kind. The expandable menu
/// lists all devices of the same kind, allowing the user to switch defaults.
pub struct AudioSlidersComponent {
    container: gtk::Box,
    #[allow(dead_code)]
    sliders: Rc<RefCell<HashMap<String, SliderEntry>>>,
}

impl AudioSlidersComponent {
    pub fn new(
        store: &Rc<EntityStore>,
        action_callback: &EntityActionCallback,
        menu_store: &Rc<waft_core::menu_state::MenuStore>,
    ) -> Self {
        let container = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .spacing(8)
            .visible(false)
            .build();

        let sliders: Rc<RefCell<HashMap<String, SliderEntry>>> =
            Rc::new(RefCell::new(HashMap::new()));

        let store_ref = store.clone();
        let container_ref = container.clone();
        let sliders_ref = sliders.clone();
        let cb = action_callback.clone();
        let menu_store_ref = menu_store.clone();

        store.subscribe_type(entity::audio::ENTITY_TYPE, move || {
            let entities: Vec<(Urn, entity::audio::AudioDevice)> =
                store_ref.get_entities_typed(entity::audio::ENTITY_TYPE);

            // Separate devices by kind
            let output_devices: Vec<&(Urn, entity::audio::AudioDevice)> = entities
                .iter()
                .filter(|(_, d)| d.kind == AudioDeviceKind::Output)
                .collect();
            let input_devices: Vec<&(Urn, entity::audio::AudioDevice)> = entities
                .iter()
                .filter(|(_, d)| d.kind == AudioDeviceKind::Input)
                .collect();

            let mut sliders = sliders_ref.borrow_mut();
            let mut needs_sort = false;

            // Process each kind: output and input
            for (kind_key, devices) in [("output", &output_devices), ("input", &input_devices)] {
                let kind = if kind_key == "output" {
                    AudioDeviceKind::Output
                } else {
                    AudioDeviceKind::Input
                };

                // Find the default device for this kind
                let default_device = devices.iter().find(|(_, d)| d.default);

                if let Some((default_urn, default_dev)) = default_device {
                    let icon = slider_icon(default_dev);
                    let has_multiple = devices.len() > 1;
                    let menu_id = menu_id_for_widget(&format!("audio-{kind_key}"));
                    let props = AudioSliderMenuProps {
                        icon,
                        value: default_dev.volume,
                        disabled: default_dev.muted,
                        expandable: has_multiple,
                        expanded: is_menu_open(&menu_store_ref, &menu_id),
                        devices: build_device_entries(devices),
                    };

                    if let Some(entry) = sliders.get(kind_key) {
                        // Update existing entry with default device state
                        entry.menu.update(&props);
                        *entry.current_props.borrow_mut() = props;

                        // Update the current URN so callbacks use the right device
                        *entry.current_urn.borrow_mut() = Some((*default_urn).clone());

                        // Update throttle callback for the new default device
                        let urn_for_drag = (*default_urn).clone();
                        let cb_drag = cb.clone();
                        entry.throttle.set_callback(move |v| {
                            cb_drag(
                                urn_for_drag.clone(),
                                "set-volume".to_string(),
                                serde_json::json!({ "value": v }),
                            );
                        });
                    } else {
                        // Create new slider entry for this kind
                        let menu_id = menu_id_for_widget(&format!("audio-{kind_key}"));
                        let current_props = Rc::new(RefCell::new(props.clone()));
                        let menu = Rc::new(AudioSliderMenu::build(&props));

                        // Track the current default device URN
                        let current_urn: Rc<RefCell<Option<Urn>>> =
                            Rc::new(RefCell::new(Some((*default_urn).clone())));

                        // Wire value_change -> throttled set-volume during drag
                        let throttle = ThrottledSender::new(Duration::from_millis(50));
                        let urn_for_drag = (*default_urn).clone();
                        let cb_drag = cb.clone();
                        throttle.set_callback(move |v| {
                            cb_drag(
                                urn_for_drag.clone(),
                                "set-volume".to_string(),
                                serde_json::json!({ "value": v }),
                            );
                        });
                        let throttle_fn = throttle.throttle_fn();

                        // Wire output events
                        let current_urn_out = current_urn.clone();
                        let cb_out = cb.clone();
                        let menu_store_out = menu_store_ref.clone();
                        let menu_id_out = menu_id.clone();
                        menu.connect_output(move |output| match output {
                            AudioSliderMenuOutput::ValueChanged(v) => {
                                throttle_fn(v);
                            }
                            AudioSliderMenuOutput::ValueCommit(v) => {
                                if let Some(ref urn) = *current_urn_out.borrow() {
                                    cb_out(
                                        urn.clone(),
                                        "set-volume".to_string(),
                                        serde_json::json!({ "value": v }),
                                    );
                                }
                            }
                            AudioSliderMenuOutput::IconClick => {
                                if let Some(ref urn) = *current_urn_out.borrow() {
                                    cb_out(
                                        urn.clone(),
                                        "toggle-mute".to_string(),
                                        serde_json::Value::Null,
                                    );
                                }
                            }
                            AudioSliderMenuOutput::ExpandClick => {
                                toggle_menu(&menu_store_out, &menu_id_out);
                            }
                            AudioSliderMenuOutput::SelectDevice(urn) => {
                                cb_out(urn, "set-default".to_string(), serde_json::Value::Null);
                            }
                        });

                        // Subscribe to MenuStore to sync expand state
                        let store_sub = menu_store_ref.clone();
                        let mid = menu_id.clone();
                        let menu_for_sub = Rc::clone(&menu);
                        let props_for_sub = Rc::clone(&current_props);
                        menu_store_ref.subscribe(move || {
                            let state = store_sub.get_state();
                            let open = state.active_menu_id.as_deref() == Some(mid.as_str());
                            let mut current = props_for_sub.borrow_mut();
                            if current.expanded != open {
                                current.expanded = open;
                                menu_for_sub.update(&*current);
                            }
                        });

                        container_ref.append(&menu.widget());

                        sliders.insert(
                            kind_key.to_string(),
                            SliderEntry {
                                menu,
                                kind,
                                throttle,
                                current_urn,
                                current_props,
                            },
                        );
                        needs_sort = true;
                    }
                } else if devices.is_empty() {
                    // No devices at all for this kind -- remove the slider
                    if let Some(entry) = sliders.remove(kind_key) {
                        container_ref.remove(&entry.menu.widget());
                        needs_sort = true;
                    }
                }
                // When devices exist but none is marked default, keep the
                // existing slider as-is. This avoids destroying and recreating
                // the widget (and its MenuStore subscription) during the
                // transient state between individual entity updates when the
                // default device changes.
            }

            // Re-sort children only when the set of slider kinds changed (a slider
            // was added or removed). Re-parenting widgets during a drag kills the
            // scale's internal gesture, so this must never run on routine updates.
            if needs_sort {
                while let Some(child) = container_ref.first_child() {
                    container_ref.remove(&child);
                }

                let mut sorted: Vec<(&String, &SliderEntry)> = sliders.iter().collect();
                sorted.sort_by_key(|(_, entry)| match entry.kind {
                    AudioDeviceKind::Output => 60,
                    AudioDeviceKind::Input => 65,
                });

                for (_, entry) in &sorted {
                    container_ref.append(&entry.menu.widget());
                }

                container_ref.set_visible(!sliders.is_empty());
            } else {
                container_ref.set_visible(!sliders.is_empty());
            }
        });

        Self { container, sliders }
    }

    pub fn widget(&self) -> &gtk::Widget {
        self.container.upcast_ref()
    }
}

/// Build the device list for `AudioSliderMenuProps` from raw entity data.
fn build_device_entries(devices: &[&(Urn, entity::audio::AudioDevice)]) -> Vec<AudioSliderDevice> {
    devices
        .iter()
        .map(|(urn, device)| AudioSliderDevice {
            urn: (*urn).clone(),
            props: AudioDeviceRowProps {
                device_type: device.device_type.clone(),
                connection_type: device.connection_type.clone(),
                kind: device.kind,
                name: device.name.clone(),
                active: device.default,
            },
        })
        .collect()
}

/// Compute the volume-level icon for an audio device slider.
///
/// The slider icon reflects the current volume level and mute state rather than
/// the device type. This keeps the device-type icon stable on the device itself
/// while the slider icon dynamically tracks volume changes.
fn slider_icon(device: &entity::audio::AudioDevice) -> String {
    match device.kind {
        AudioDeviceKind::Output => {
            if device.muted || device.volume < 0.01 {
                "audio-volume-muted-symbolic".to_string()
            } else if device.volume < 0.34 {
                "audio-volume-low-symbolic".to_string()
            } else if device.volume < 0.67 {
                "audio-volume-medium-symbolic".to_string()
            } else {
                "audio-volume-high-symbolic".to_string()
            }
        }
        AudioDeviceKind::Input => {
            if device.muted {
                "microphone-disabled-symbolic".to_string()
            } else if device.volume < 0.01 {
                "audio-input-microphone-symbolic".to_string()
            } else if device.volume < 0.34 {
                "microphone-sensitivity-low-symbolic".to_string()
            } else if device.volume < 0.67 {
                "microphone-sensitivity-medium-symbolic".to_string()
            } else {
                "microphone-sensitivity-high-symbolic".to_string()
            }
        }
    }
}
