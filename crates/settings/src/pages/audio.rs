//! Audio settings page -- smart container.
//!
//! Subscribes to `EntityStore` for `audio-card` entity type. On entity
//! changes, reconciles `AudioDeviceCard` widgets per physical audio card.
//! Also subscribes to `audio-device` for backward compatibility (the overview
//! still uses it), but the settings page UI is card-based.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use gtk::prelude::*;
use waft_client::{EntityActionCallback, EntityStore};
use waft_protocol::Urn;
use waft_protocol::entity::audio::{self, AudioCard, AudioDevice};

use crate::audio::device_card::{AudioDeviceCard, AudioDeviceCardOutput};
use crate::audio::virtual_devices_section::VirtualDevicesSection;
use crate::i18n::t;
use crate::search_index::SearchIndex;

/// Smart container for the Audio settings page.
///
/// Owns card widgets keyed by URN. Subscribes to EntityStore
/// and updates widgets when entity data changes.
pub struct AudioPage {
    pub root: gtk::Box,
}

/// Internal mutable state for the Audio page.
struct AudioPageState {
    cards: HashMap<String, AudioDeviceCard>,
    cards_box: gtk::Box,
    empty_state: adw::StatusPage,
    action_callback: EntityActionCallback,
}

impl AudioPage {
    pub fn new(
        entity_store: &Rc<EntityStore>,
        action_callback: &EntityActionCallback,
        search_index: &Rc<RefCell<SearchIndex>>,
    ) -> Self {
        let root = crate::page_layout::page_root();

        let cards_box = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .spacing(24)
            .build();
        root.append(&cards_box);

        // Virtual devices section
        let virtual_section = VirtualDevicesSection::new(action_callback);
        root.append(&virtual_section.root);

        // Empty state
        let empty_state = adw::StatusPage::builder()
            .icon_name("audio-speakers-symbolic")
            .title(t("audio-no-devices"))
            .description(t("audio-no-devices-desc"))
            .build();
        root.append(&empty_state);

        // Register search entry
        {
            let mut idx = search_index.borrow_mut();
            let page_title = t("settings-audio");
            idx.add_section(
                "audio",
                &page_title,
                &t("audio-output-devices"),
                "audio-output-devices",
                &cards_box,
            );
        }

        let state = Rc::new(RefCell::new(AudioPageState {
            cards: HashMap::new(),
            cards_box,
            empty_state,
            action_callback: action_callback.clone(),
        }));

        // Subscribe to audio card changes (future updates + initial reconciliation)
        crate::subscription::subscribe_entities::<AudioCard, _>(
            entity_store,
            audio::CARD_ENTITY_TYPE,
            {
                let state = state.clone();
                move |cards| {
                    log::debug!(
                        "[audio-page] Reconciling: {} cards",
                        cards.len()
                    );
                    Self::reconcile(&state, &cards);
                }
            },
        );

        // Subscribe to audio-device for virtual device updates
        crate::subscription::subscribe_entities::<AudioDevice, _>(
            entity_store,
            audio::ENTITY_TYPE,
            move |devices| {
                virtual_section.reconcile(&devices);
            },
        );

        Self { root }
    }

    /// Reconcile card widgets with current card data.
    fn reconcile(state: &Rc<RefCell<AudioPageState>>, cards: &[(Urn, AudioCard)]) {
        let mut state = state.borrow_mut();
        let mut seen = std::collections::HashSet::new();

        for (urn, card) in cards {
            let urn_str = urn.as_str().to_string();
            seen.insert(urn_str.clone());

            if let Some(existing) = state.cards.get(&urn_str) {
                existing.apply_props(card);
            } else {
                let widget = AudioDeviceCard::new(card);
                let urn_clone = urn.clone();
                let cb = state.action_callback.clone();
                widget.connect_output(move |output| {
                    let (action, params) = match output {
                        AudioDeviceCardOutput::SetProfile(profile) => {
                            ("set-profile", serde_json::json!({ "profile": profile }))
                        }
                        AudioDeviceCardOutput::SetSinkVolume { sink, volume } => (
                            "set-volume",
                            serde_json::json!({ "sink": sink, "value": volume }),
                        ),
                        AudioDeviceCardOutput::ToggleSinkMute { sink } => {
                            ("toggle-mute", serde_json::json!({ "sink": sink }))
                        }
                        AudioDeviceCardOutput::SetSinkDefault { sink } => {
                            ("set-default", serde_json::json!({ "sink": sink }))
                        }
                        AudioDeviceCardOutput::SetSinkPort { sink, port } => (
                            "set-sink-port",
                            serde_json::json!({ "sink": sink, "port": port }),
                        ),
                        AudioDeviceCardOutput::SetSourceVolume { source, volume } => (
                            "set-volume",
                            serde_json::json!({ "source": source, "value": volume }),
                        ),
                        AudioDeviceCardOutput::ToggleSourceMute { source } => {
                            ("toggle-mute", serde_json::json!({ "source": source }))
                        }
                        AudioDeviceCardOutput::SetSourceDefault { source } => {
                            ("set-default", serde_json::json!({ "source": source }))
                        }
                        AudioDeviceCardOutput::SetSourcePort { source, port } => (
                            "set-source-port",
                            serde_json::json!({ "source": source, "port": port }),
                        ),
                    };
                    cb(urn_clone.clone(), action.to_string(), params);
                });
                state.cards_box.append(&widget.root);
                state.cards.insert(urn_str, widget);
            }
        }

        // Remove cards no longer present
        let to_remove: Vec<String> = state
            .cards
            .keys()
            .filter(|k| !seen.contains(*k))
            .cloned()
            .collect();

        for key in to_remove {
            if let Some(card) = state.cards.remove(&key) {
                state.cards_box.remove(&card.root);
            }
        }

        // Show empty state only when no cards
        state.empty_state.set_visible(cards.is_empty());
    }
}
