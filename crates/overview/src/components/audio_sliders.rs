//! Audio device sliders component.
//!
//! Subscribes to the `audio-device` entity type and renders expandable sliders
//! for default output and input devices. Each slider has a menu listing all
//! devices of that kind, allowing the user to switch the default device.
//! Output devices sort before input devices (weight 60 vs 65).

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use gtk::prelude::*;
use waft_protocol::entity;
use waft_protocol::entity::audio::AudioDeviceKind;
use waft_protocol::Urn;
use waft_ui_gtk::menu_state::menu_id_for_widget;
use waft_ui_gtk::audio::device_row::{AudioDeviceRow, AudioDeviceRowOutput, AudioDeviceRowProps};
use waft_ui_gtk::widgets::slider::{SliderProps, SliderWidget};

use crate::entity_store::{EntityActionCallback, EntityStore};

/// A device row in the expandable menu.
struct DeviceMenuRow {
    urn_str: String,
    row: Rc<AudioDeviceRow>,
}

/// A slider entry keyed by device kind (output or input).
struct SliderEntry {
    widget: Rc<SliderWidget>,
    kind: AudioDeviceKind,
    menu_revealer: gtk::Revealer,
    menu_box: gtk::Box,
    device_rows: RefCell<Vec<DeviceMenuRow>>,
    /// The outer box wrapping slider + revealer.
    wrapper: gtk::Box,
}

/// Renders volume sliders for default audio output and input devices with
/// expandable device selection menus.
///
/// Each slider controls the default device of its kind. The expandable menu
/// lists all devices of the same kind, allowing the user to switch defaults.
pub struct AudioSlidersComponent {
    container: gtk::Box,
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

                    if let Some(entry) = sliders.get(kind_key) {
                        // Update existing slider with default device state
                        entry.widget.set_value(default_dev.volume);
                        entry.widget.set_disabled(default_dev.muted);
                        entry.widget.set_icon(&icon);
                        entry.widget.set_expandable(has_multiple);

                        // Reconnect value_change and icon_click to the new default URN
                        let urn_for_value = (*default_urn).clone();
                        let cb_value = cb.clone();
                        entry.widget.connect_value_change(move |v| {
                            cb_value(
                                urn_for_value.clone(),
                                "set-volume".to_string(),
                                serde_json::json!({ "value": v }),
                            );
                        });

                        let urn_for_mute = (*default_urn).clone();
                        let cb_mute = cb.clone();
                        entry.widget.connect_icon_click(move || {
                            cb_mute(
                                urn_for_mute.clone(),
                                "toggle-mute".to_string(),
                                serde_json::Value::Null,
                            );
                        });

                        // Update device menu rows
                        update_device_rows(entry, devices, &cb);
                    } else {
                        // Create new slider entry for this kind
                        let menu_id = menu_id_for_widget(&format!("audio-{kind_key}"));

                        let slider = Rc::new(SliderWidget::new(
                            SliderProps {
                                icon,
                                value: default_dev.volume,
                                disabled: default_dev.muted,
                                expandable: has_multiple,
                                menu_id: Some(menu_id.clone()),
                            },
                            Some(menu_store_ref.clone()),
                        ));

                        // Wire value_change -> set-volume action
                        let urn_for_value = (*default_urn).clone();
                        let cb_value = cb.clone();
                        slider.connect_value_change(move |v| {
                            cb_value(
                                urn_for_value.clone(),
                                "set-volume".to_string(),
                                serde_json::json!({ "value": v }),
                            );
                        });

                        // Wire icon_click -> toggle-mute action
                        let urn_for_mute = (*default_urn).clone();
                        let cb_mute = cb.clone();
                        slider.connect_icon_click(move || {
                            cb_mute(
                                urn_for_mute.clone(),
                                "toggle-mute".to_string(),
                                serde_json::Value::Null,
                            );
                        });

                        // Menu content box
                        let menu_box = gtk::Box::builder()
                            .orientation(gtk::Orientation::Vertical)
                            .spacing(0)
                            .css_classes(["menu-content"])
                            .build();

                        // Revealer for device list
                        let menu_revealer = gtk::Revealer::builder()
                            .transition_type(gtk::RevealerTransitionType::SlideDown)
                            .transition_duration(200)
                            .reveal_child(false)
                            .build();
                        menu_revealer.set_child(Some(&menu_box));

                        // Subscribe to MenuStore to show/hide the revealer
                        let store_sub = menu_store_ref.clone();
                        let mid = menu_id.clone();
                        let rev = menu_revealer.clone();
                        menu_store_ref.subscribe(move || {
                            let state = store_sub.get_state();
                            let open =
                                state.active_menu_id.as_deref() == Some(mid.as_str());
                            rev.set_reveal_child(open);
                        });

                        // Wrapper box for slider + revealer
                        let wrapper = gtk::Box::builder()
                            .orientation(gtk::Orientation::Vertical)
                            .spacing(0)
                            .build();
                        wrapper.append(&slider.widget());
                        wrapper.append(&menu_revealer);

                        let entry = SliderEntry {
                            widget: slider,
                            kind,
                            menu_revealer,
                            menu_box,
                            device_rows: RefCell::new(Vec::new()),
                            wrapper,
                        };

                        // Populate device rows
                        update_device_rows(&entry, devices, &cb);

                        sliders.insert(kind_key.to_string(), entry);
                    }
                } else if devices.is_empty() {
                    // No devices at all for this kind -- remove the slider
                    if let Some(entry) = sliders.remove(kind_key) {
                        container_ref.remove(&entry.wrapper);
                    }
                }
                // When devices exist but none is marked default, keep the
                // existing slider as-is. This avoids destroying and recreating
                // the widget (and its MenuStore subscription) during the
                // transient state between individual entity updates when the
                // default device changes.
            }

            // Re-sort children: output (weight 60) before input (weight 65)
            while let Some(child) = container_ref.first_child() {
                container_ref.remove(&child);
            }

            let mut sorted: Vec<(&String, &SliderEntry)> = sliders.iter().collect();
            sorted.sort_by_key(|(_, entry)| match entry.kind {
                AudioDeviceKind::Output => 60,
                AudioDeviceKind::Input => 65,
            });

            for (_, entry) in &sorted {
                container_ref.append(&entry.wrapper);
            }

            container_ref.set_visible(!sliders.is_empty());
        });

        Self { container, sliders }
    }

    pub fn widget(&self) -> &gtk::Widget {
        self.container.upcast_ref()
    }
}

/// Update device menu rows for a slider entry.
fn update_device_rows(
    entry: &SliderEntry,
    devices: &[&(Urn, entity::audio::AudioDevice)],
    action_callback: &EntityActionCallback,
) {
    let mut rows = entry.device_rows.borrow_mut();

    // Collect current device URN strings
    let current_urns: Vec<String> = devices
        .iter()
        .map(|(urn, _)| urn.as_str().to_string())
        .collect();

    // Remove stale rows
    rows.retain(|row| {
        if current_urns.contains(&row.urn_str) {
            true
        } else {
            row.row.root.unparent();
            false
        }
    });

    // Update existing or create new rows
    for (urn, device) in devices {
        let urn_str = urn.as_str().to_string();

        if let Some(existing) = rows.iter().find(|r| r.urn_str == urn_str) {
            // Update in place
            existing.row.set_name(&device.name);
            existing.row.set_device_icon(&device.icon);
            existing.row.set_connection_icon(device.connection_icon.as_deref());
            existing.row.set_active(device.default);
        } else {
            // Create new row
            let device_row = Rc::new(AudioDeviceRow::new(AudioDeviceRowProps {
                device_icon: device.icon.clone(),
                connection_icon: device.connection_icon.clone(),
                name: device.name.clone(),
                active: device.default,
            }));

            let action_cb = action_callback.clone();
            let urn_for_action = (*urn).clone();
            device_row.connect_output(move |AudioDeviceRowOutput::SelectAsDefault| {
                action_cb(
                    urn_for_action.clone(),
                    "set-default".to_string(),
                    serde_json::Value::Null,
                );
            });

            entry.menu_box.append(&device_row.root);
            rows.push(DeviceMenuRow {
                urn_str,
                row: device_row,
            });
        }
    }
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
