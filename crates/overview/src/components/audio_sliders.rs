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
///
/// Uses [`deduplicate_device_names`] to disambiguate devices that share the
/// same name (e.g. two "Built-in Audio" entries on different connection types).
fn build_device_entries(devices: &[&(Urn, entity::audio::AudioDevice)]) -> Vec<AudioSliderDevice> {
    let pairs: Vec<(Urn, entity::audio::AudioDevice)> = devices
        .iter()
        .map(|(u, d)| ((*u).clone(), d.clone()))
        .collect();
    let display_names = deduplicate_device_names(&pairs);

    devices
        .iter()
        .zip(display_names)
        .map(|((urn, device), (_, display_label))| AudioSliderDevice {
            urn: (*urn).clone(),
            props: AudioDeviceRowProps {
                device_type: device.device_type.clone(),
                connection_type: device.connection_type.clone(),
                kind: device.kind,
                name: display_label,
                active: device.default,
            },
        })
        .collect()
}

/// Produces display labels for a list of audio devices, disambiguating
/// devices that share the same `name` by appending a connection-type suffix.
///
/// Unique names pass through unchanged. Colliding names become
/// `"{name} ({TYPE} {n})"` where `TYPE` is `connection_type` uppercased
/// (or `"DEVICE"` when absent) and `n` is a 1-based counter within that name
/// group.
fn deduplicate_device_names(
    devices: &[(Urn, entity::audio::AudioDevice)],
) -> Vec<(Urn, String)> {
    // Count how many devices share each name.
    let mut name_counts: HashMap<&str, usize> = HashMap::new();
    for (_, device) in devices {
        *name_counts.entry(&device.name).or_insert(0) += 1;
    }

    // Assign display labels: unique names stay as-is, duplicates get a suffix.
    let mut name_counters: HashMap<&str, usize> = HashMap::new();
    devices
        .iter()
        .map(|(urn, device)| {
            let count = name_counts.get(device.name.as_str()).copied().unwrap_or(1);
            let label = if count > 1 {
                let n = name_counters.entry(&device.name).or_insert(0);
                *n += 1;
                let conn = device
                    .connection_type
                    .as_deref()
                    .unwrap_or("Device")
                    .to_uppercase();
                format!("{} ({} {})", device.name, conn, n)
            } else {
                device.name.clone()
            };
            (urn.clone(), label)
        })
        .collect()
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use entity::audio::{AudioDevice, AudioDeviceKind};

    fn make_device(name: &str, connection_type: Option<&str>) -> AudioDevice {
        AudioDevice {
            name: name.to_string(),
            device_type: "card".to_string(),
            connection_type: connection_type.map(|s| s.to_string()),
            volume: 0.5,
            muted: false,
            default: false,
            kind: AudioDeviceKind::Output,
            virtual_device: false,
            sink_name: None,
        }
    }

    fn make_urn(id: &str) -> Urn {
        Urn::parse(id).expect("valid URN")
    }

    #[test]
    fn unique_names_unchanged() {
        let devices = vec![
            (make_urn("audio/audio-device/speakers"), make_device("Speakers", Some("jack"))),
            (make_urn("audio/audio-device/headset"), make_device("Headset", Some("bluetooth"))),
        ];
        let result = deduplicate_device_names(&devices);
        assert_eq!(result[0].1, "Speakers");
        assert_eq!(result[1].1, "Headset");
    }

    #[test]
    fn duplicates_with_connection_type() {
        let devices = vec![
            (make_urn("audio/audio-device/builtin1"), make_device("Built-in Audio", Some("jack"))),
            (make_urn("audio/audio-device/builtin2"), make_device("Built-in Audio", Some("hdmi"))),
        ];
        let result = deduplicate_device_names(&devices);
        assert_eq!(result[0].1, "Built-in Audio (JACK 1)");
        assert_eq!(result[1].1, "Built-in Audio (HDMI 2)");
    }

    #[test]
    fn duplicates_without_connection_type() {
        let devices = vec![
            (make_urn("audio/audio-device/dev1"), make_device("Monitor", None)),
            (make_urn("audio/audio-device/dev2"), make_device("Monitor", None)),
        ];
        let result = deduplicate_device_names(&devices);
        assert_eq!(result[0].1, "Monitor (DEVICE 1)");
        assert_eq!(result[1].1, "Monitor (DEVICE 2)");
    }

    fn make_device_with_volume(kind: AudioDeviceKind, volume: f64, muted: bool) -> AudioDevice {
        AudioDevice {
            name: "Test".to_string(),
            device_type: "card".to_string(),
            connection_type: None,
            volume,
            muted,
            default: false,
            kind,
            virtual_device: false,
            sink_name: None,
        }
    }

    #[test]
    fn slider_icon_output_muted() {
        let dev = make_device_with_volume(AudioDeviceKind::Output, 0.8, true);
        assert_eq!(slider_icon(&dev), "audio-volume-muted-symbolic");
    }

    #[test]
    fn slider_icon_output_zero_volume() {
        let dev = make_device_with_volume(AudioDeviceKind::Output, 0.0, false);
        assert_eq!(slider_icon(&dev), "audio-volume-muted-symbolic");
    }

    #[test]
    fn slider_icon_output_low_volume() {
        let dev = make_device_with_volume(AudioDeviceKind::Output, 0.2, false);
        assert_eq!(slider_icon(&dev), "audio-volume-low-symbolic");
    }

    #[test]
    fn slider_icon_output_medium_volume() {
        let dev = make_device_with_volume(AudioDeviceKind::Output, 0.5, false);
        assert_eq!(slider_icon(&dev), "audio-volume-medium-symbolic");
    }

    #[test]
    fn slider_icon_output_high_volume() {
        let dev = make_device_with_volume(AudioDeviceKind::Output, 0.9, false);
        assert_eq!(slider_icon(&dev), "audio-volume-high-symbolic");
    }

    #[test]
    fn slider_icon_input_muted() {
        let dev = make_device_with_volume(AudioDeviceKind::Input, 0.8, true);
        assert_eq!(slider_icon(&dev), "microphone-disabled-symbolic");
    }

    #[test]
    fn slider_icon_input_zero_volume() {
        let dev = make_device_with_volume(AudioDeviceKind::Input, 0.0, false);
        assert_eq!(slider_icon(&dev), "audio-input-microphone-symbolic");
    }

    #[test]
    fn slider_icon_input_low_volume() {
        let dev = make_device_with_volume(AudioDeviceKind::Input, 0.2, false);
        assert_eq!(slider_icon(&dev), "microphone-sensitivity-low-symbolic");
    }

    #[test]
    fn slider_icon_input_medium_volume() {
        let dev = make_device_with_volume(AudioDeviceKind::Input, 0.5, false);
        assert_eq!(slider_icon(&dev), "microphone-sensitivity-medium-symbolic");
    }

    #[test]
    fn slider_icon_input_high_volume() {
        let dev = make_device_with_volume(AudioDeviceKind::Input, 0.9, false);
        assert_eq!(slider_icon(&dev), "microphone-sensitivity-high-symbolic");
    }

    #[test]
    fn slider_icon_output_boundary_low_medium() {
        // At the boundary 0.34, should be medium
        let dev = make_device_with_volume(AudioDeviceKind::Output, 0.34, false);
        assert_eq!(slider_icon(&dev), "audio-volume-medium-symbolic");
    }

    #[test]
    fn slider_icon_output_boundary_medium_high() {
        // At the boundary 0.67, should be high
        let dev = make_device_with_volume(AudioDeviceKind::Output, 0.67, false);
        assert_eq!(slider_icon(&dev), "audio-volume-high-symbolic");
    }

    // --- GTK component lifecycle tests ---

    use waft_protocol::message::AppNotification;

    fn make_audio_entity(
        kind: AudioDeviceKind,
        volume: f64,
        default: bool,
    ) -> entity::audio::AudioDevice {
        AudioDevice {
            name: "Test Device".to_string(),
            device_type: "card".to_string(),
            connection_type: Some("analog".to_string()),
            volume,
            muted: false,
            default,
            kind,
            virtual_device: false,
            sink_name: None,
        }
    }

    fn make_audio_updated(urn: Urn, data: serde_json::Value) -> AppNotification {
        AppNotification::EntityUpdated {
            urn,
            entity_type: entity::audio::ENTITY_TYPE.to_string(),
            data,
        }
    }

    fn make_audio_removed(urn: Urn) -> AppNotification {
        AppNotification::EntityRemoved {
            urn,
            entity_type: entity::audio::ENTITY_TYPE.to_string(),
        }
    }

    fn noop_action_callback() -> EntityActionCallback {
        Rc::new(|_urn, _action, _params| {})
    }

    fn child_count(container: &gtk::Widget) -> u32 {
        let bx: &gtk::Box = container.downcast_ref().unwrap();
        let mut count = 0u32;
        let mut child = bx.first_child();
        while let Some(c) = child {
            count += 1;
            child = c.next_sibling();
        }
        count
    }

    /// Run all audio slider GTK tests. Called from the single GTK test entry point.
    pub(crate) fn run_all_gtk() {
        test_container_starts_invisible();
        test_add_default_output_makes_visible();
        test_add_output_and_input();
        test_update_default_device_preserves_child_count();
        test_remove_all_makes_invisible();
        test_non_default_devices_alone_do_not_create_slider();
    }

    fn test_container_starts_invisible() {
        let store = Rc::new(EntityStore::new());
        let menu_store = Rc::new(waft_core::menu_state::create_menu_store());
        let comp = AudioSlidersComponent::new(&store, &noop_action_callback(), &menu_store);
        assert!(!comp.widget().is_visible(), "container should start invisible");
        assert_eq!(child_count(comp.widget()), 0);
    }

    fn test_add_default_output_makes_visible() {
        let store = Rc::new(EntityStore::new());
        let menu_store = Rc::new(waft_core::menu_state::create_menu_store());
        let comp = AudioSlidersComponent::new(&store, &noop_action_callback(), &menu_store);

        let urn = Urn::new("audio", "audio-device", "speakers");
        let data = serde_json::to_value(make_audio_entity(AudioDeviceKind::Output, 0.75, true)).unwrap();
        store.handle_notification(make_audio_updated(urn, data));

        assert!(comp.widget().is_visible(), "container should be visible with default output");
        assert_eq!(child_count(comp.widget()), 1);
    }

    fn test_add_output_and_input() {
        let store = Rc::new(EntityStore::new());
        let menu_store = Rc::new(waft_core::menu_state::create_menu_store());
        let comp = AudioSlidersComponent::new(&store, &noop_action_callback(), &menu_store);

        // Add default output
        let urn_out = Urn::new("audio", "audio-device", "speakers");
        let data_out = serde_json::to_value(make_audio_entity(AudioDeviceKind::Output, 0.75, true)).unwrap();
        store.handle_notification(make_audio_updated(urn_out, data_out));

        // Add default input
        let urn_in = Urn::new("audio", "audio-device", "mic");
        let data_in = serde_json::to_value(make_audio_entity(AudioDeviceKind::Input, 0.50, true)).unwrap();
        store.handle_notification(make_audio_updated(urn_in, data_in));

        assert_eq!(child_count(comp.widget()), 2, "should have output + input sliders");
    }

    fn test_update_default_device_preserves_child_count() {
        let store = Rc::new(EntityStore::new());
        let menu_store = Rc::new(waft_core::menu_state::create_menu_store());
        let comp = AudioSlidersComponent::new(&store, &noop_action_callback(), &menu_store);

        let urn = Urn::new("audio", "audio-device", "speakers");
        let data1 = serde_json::to_value(make_audio_entity(AudioDeviceKind::Output, 0.75, true)).unwrap();
        store.handle_notification(make_audio_updated(urn.clone(), data1));
        assert_eq!(child_count(comp.widget()), 1);

        // Update volume
        let data2 = serde_json::to_value(make_audio_entity(AudioDeviceKind::Output, 0.30, true)).unwrap();
        store.handle_notification(make_audio_updated(urn, data2));
        assert_eq!(child_count(comp.widget()), 1, "update should not create new widget");
    }

    fn test_remove_all_makes_invisible() {
        let store = Rc::new(EntityStore::new());
        let menu_store = Rc::new(waft_core::menu_state::create_menu_store());
        let comp = AudioSlidersComponent::new(&store, &noop_action_callback(), &menu_store);

        let urn = Urn::new("audio", "audio-device", "speakers");
        let data = serde_json::to_value(make_audio_entity(AudioDeviceKind::Output, 0.75, true)).unwrap();
        store.handle_notification(make_audio_updated(urn.clone(), data));
        assert!(comp.widget().is_visible());

        store.handle_notification(make_audio_removed(urn));
        assert!(!comp.widget().is_visible(), "container should be invisible when empty");
        assert_eq!(child_count(comp.widget()), 0);
    }

    fn test_non_default_devices_alone_do_not_create_slider() {
        let store = Rc::new(EntityStore::new());
        let menu_store = Rc::new(waft_core::menu_state::create_menu_store());
        let comp = AudioSlidersComponent::new(&store, &noop_action_callback(), &menu_store);

        // Add a non-default output device
        let urn = Urn::new("audio", "audio-device", "headphones");
        let data = serde_json::to_value(make_audio_entity(AudioDeviceKind::Output, 0.60, false)).unwrap();
        store.handle_notification(make_audio_updated(urn, data));

        // Audio sliders only render for default devices
        assert_eq!(child_count(comp.widget()), 0, "non-default device should not create slider");
        assert!(!comp.widget().is_visible());
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
