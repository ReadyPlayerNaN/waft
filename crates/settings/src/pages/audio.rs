//! Audio settings page -- smart container.
//!
//! Subscribes to `EntityStore` for `audio-device` entity type. On entity
//! changes, partitions devices into output and input groups and reconciles
//! the corresponding `AudioDeviceGroup` widgets.

use std::cell::RefCell;
use std::rc::Rc;

use gtk::prelude::*;
use waft_client::{EntityActionCallback, EntityStore};
use waft_protocol::Urn;
use waft_protocol::entity::audio::{self, AudioDevice, AudioDeviceKind};

use crate::audio::device_group::AudioDeviceGroup;
use crate::audio::device_row::AudioDeviceRowOutput;
use crate::i18n::t;
use crate::search_index::SearchIndex;

/// Smart container for the Audio settings page.
///
/// Owns output and input device groups. Subscribes to EntityStore
/// and updates widgets when entity data changes.
pub struct AudioPage {
    pub root: gtk::Box,
}

/// Internal mutable state for the Audio page.
struct AudioPageState {
    output_group: AudioDeviceGroup,
    input_group: AudioDeviceGroup,
    empty_state: adw::StatusPage,
}

impl AudioPage {
    pub fn new(
        entity_store: &Rc<EntityStore>,
        action_callback: &EntityActionCallback,
        search_index: &Rc<RefCell<SearchIndex>>,
    ) -> Self {
        let root = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .spacing(24)
            .margin_top(24)
            .margin_bottom(24)
            .margin_start(12)
            .margin_end(12)
            .build();

        // Output devices group
        let output_group = AudioDeviceGroup::new(&t("audio-output-devices"));
        output_group.root.set_visible(false);
        root.append(&output_group.root);

        // Input devices group
        let input_group = AudioDeviceGroup::new(&t("audio-input-devices"));
        input_group.root.set_visible(false);
        root.append(&input_group.root);

        // Empty state
        let empty_state = adw::StatusPage::builder()
            .icon_name("audio-speakers-symbolic")
            .title(t("audio-no-devices"))
            .description(t("audio-no-devices-desc"))
            .build();
        root.append(&empty_state);

        // Register search entries
        {
            let mut idx = search_index.borrow_mut();
            let page_title = t("settings-audio");
            idx.add_section("audio", &page_title, &t("audio-output-devices"), "audio-output-devices", &output_group.root);
            idx.add_section("audio", &page_title, &t("audio-input-devices"), "audio-input-devices", &input_group.root);
        }

        // Wire output group actions
        {
            let cb = action_callback.clone();
            output_group.connect_output(move |output| {
                let (action, params) = match output.action {
                    AudioDeviceRowOutput::SetVolume(v) => {
                        ("set-volume", serde_json::json!({ "value": v }))
                    }
                    AudioDeviceRowOutput::ToggleMute => {
                        ("toggle-mute", serde_json::Value::Null)
                    }
                    AudioDeviceRowOutput::SetDefault => {
                        ("set-default", serde_json::Value::Null)
                    }
                };
                cb(output.urn, action.to_string(), params);
            });
        }

        // Wire input group actions
        {
            let cb = action_callback.clone();
            input_group.connect_output(move |output| {
                let (action, params) = match output.action {
                    AudioDeviceRowOutput::SetVolume(v) => {
                        ("set-volume", serde_json::json!({ "value": v }))
                    }
                    AudioDeviceRowOutput::ToggleMute => {
                        ("toggle-mute", serde_json::Value::Null)
                    }
                    AudioDeviceRowOutput::SetDefault => {
                        ("set-default", serde_json::Value::Null)
                    }
                };
                cb(output.urn, action.to_string(), params);
            });
        }

        let state = Rc::new(RefCell::new(AudioPageState {
            output_group,
            input_group,
            empty_state,
        }));

        // Subscribe to audio device changes
        {
            let store = entity_store.clone();
            let state = state.clone();
            entity_store.subscribe_type(audio::ENTITY_TYPE, move || {
                let devices: Vec<(Urn, AudioDevice)> =
                    store.get_entities_typed(audio::ENTITY_TYPE);
                log::debug!(
                    "[audio-page] Subscription triggered: {} devices",
                    devices.len()
                );
                Self::reconcile(&state, &devices);
            });
        }

        // Trigger initial reconciliation with current cached data
        {
            let state_clone = state.clone();
            let store_clone = entity_store.clone();

            gtk::glib::idle_add_local_once(move || {
                let devices: Vec<(Urn, AudioDevice)> =
                    store_clone.get_entities_typed(audio::ENTITY_TYPE);

                if !devices.is_empty() {
                    log::debug!(
                        "[audio-page] Initial reconciliation: {} devices",
                        devices.len()
                    );
                    Self::reconcile(&state_clone, &devices);
                }
            });
        }

        Self { root }
    }

    /// Reconcile device groups with current device data.
    fn reconcile(
        state: &Rc<RefCell<AudioPageState>>,
        devices: &[(Urn, AudioDevice)],
    ) {
        let state = state.borrow();

        let outputs: Vec<(Urn, AudioDevice)> = devices
            .iter()
            .filter(|(_, d)| d.kind == AudioDeviceKind::Output)
            .cloned()
            .collect();

        let inputs: Vec<(Urn, AudioDevice)> = devices
            .iter()
            .filter(|(_, d)| d.kind == AudioDeviceKind::Input)
            .cloned()
            .collect();

        state.output_group.reconcile(&outputs);
        state.input_group.reconcile(&inputs);

        // Show empty state only when no devices at all
        state.empty_state.set_visible(devices.is_empty());
    }
}
