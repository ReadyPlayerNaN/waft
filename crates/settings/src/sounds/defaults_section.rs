//! Sound defaults section -- smart container.
//!
//! Subscribes to `EntityStore` for `sound-config` and `notification-sound` entity types.
//! Provides master toggle and per-urgency default sound selection with dropdowns.

use std::cell::Cell;
use std::cell::RefCell;
use std::rc::Rc;

use adw::prelude::*;
use waft_client::{EntityActionCallback, EntityStore};
use crate::search_index::SearchIndex;
use waft_protocol::Urn;
use waft_protocol::entity::notification_filter::{SOUND_CONFIG_ENTITY_TYPE, SoundConfigEntity};
use waft_protocol::entity::notification_sound::{NOTIFICATION_SOUND_ENTITY_TYPE, NotificationSound};

use crate::i18n::t;

/// XDG sound theme IDs (technical names for sound lookup).
const XDG_SOUND_IDS: &[&str] = &[
    "message-new-email",
    "message-new-instant",
    "dialog-warning",
    "bell",
    "phone-incoming-call",
];

/// XDG sound display labels (translated).
fn xdg_sound_labels() -> Vec<String> {
    vec![
        t("sounds-xdg-new-email"),
        t("sounds-xdg-new-message"),
        t("sounds-xdg-warning"),
        t("sounds-xdg-bell"),
        t("sounds-xdg-incoming-call"),
    ]
}

/// Smart container for sound defaults (master toggle + per-urgency).
pub struct DefaultsSection {
    pub root: adw::PreferencesGroup,
}

impl DefaultsSection {
    /// Phase 1: Register static search entries without constructing widgets.
    pub fn register_search(idx: &mut SearchIndex) {
        let page_title = t("settings-notifications");
        let section_title = t("sounds-defaults");
        idx.add_section_deferred("notifications", &page_title, &section_title, "sounds-defaults");
        idx.add_input_deferred("notifications", &page_title, &section_title, &t("sounds-enable"), "sounds-enable");
    }

    pub fn new(
        entity_store: &Rc<EntityStore>,
        action_callback: &EntityActionCallback,
        search_index: &Rc<RefCell<SearchIndex>>,
    ) -> Self {
        let group = adw::PreferencesGroup::builder()
            .title(t("sounds-defaults"))
            .visible(false)
            .build();

        let enabled_row = adw::SwitchRow::builder()
            .title(t("sounds-enable"))
            .build();
        group.add(&enabled_row);

        // Per-urgency rows with combo + custom entry
        let low_row = Self::create_urgency_row(&t("sounds-low-urgency"));
        group.add(&low_row.combo);

        let normal_row = Self::create_urgency_row(&t("sounds-normal-urgency"));
        group.add(&normal_row.combo);

        let critical_row = Self::create_urgency_row(&t("sounds-critical-urgency"));
        group.add(&critical_row.combo);

        // Backfill search entry widgets
        {
            let mut idx = search_index.borrow_mut();
            let section = t("sounds-defaults");
            idx.backfill_widget("notifications", &section, None, Some(&group));
            idx.backfill_widget("notifications", &section, Some(&t("sounds-enable")), Some(&enabled_row));
        }

        let updating = Rc::new(Cell::new(false));
        let current_urn: Rc<RefCell<Option<Urn>>> = Rc::new(RefCell::new(None));
        let gallery_sounds: Rc<RefCell<Vec<NotificationSound>>> =
            Rc::new(RefCell::new(Vec::new()));

        // Build the send_update closure
        let send_update: Rc<dyn Fn()> = {
            let cb = action_callback.clone();
            let urn_ref = current_urn.clone();
            let enabled_ref = enabled_row.clone();
            let low_ref = low_row.clone();
            let normal_ref = normal_row.clone();
            let critical_ref = critical_row.clone();
            let gallery_ref = gallery_sounds.clone();

            Rc::new(move || {
                let Some(ref urn) = *urn_ref.borrow() else {
                    return;
                };
                let entity = SoundConfigEntity {
                    enabled: enabled_ref.is_active(),
                    default_low: resolve_urgency_value(&low_ref, &gallery_ref.borrow()),
                    default_normal: resolve_urgency_value(&normal_ref, &gallery_ref.borrow()),
                    default_critical: resolve_urgency_value(&critical_ref, &gallery_ref.borrow()),
                };
                let params = match serde_json::to_value(&entity) {
                    Ok(v) => v,
                    Err(e) => {
                        log::warn!("[sounds/defaults] failed to serialize: {e}");
                        return;
                    }
                };
                cb(urn.clone(), "update-sound-config".to_string(), params);
            })
        };

        // Preview closure
        let send_preview: Rc<dyn Fn(String)> = {
            let cb = action_callback.clone();
            Rc::new(move |reference: String| {
                let urn = Urn::new("notifications", "sound-config", "default");
                cb(
                    urn,
                    "preview-sound".to_string(),
                    serde_json::json!({ "reference": reference }),
                );
            })
        };

        // Wire enabled toggle
        {
            let guard = updating.clone();
            let send = send_update.clone();
            enabled_row.connect_active_notify(move |_row| {
                if guard.get() {
                    return;
                }
                send();
            });
        }

        // Wire urgency row combos
        Self::wire_urgency_combo(
            &low_row,
            &updating,
            &send_update,
            &send_preview,
            &gallery_sounds,
        );
        Self::wire_urgency_combo(
            &normal_row,
            &updating,
            &send_update,
            &send_preview,
            &gallery_sounds,
        );
        Self::wire_urgency_combo(
            &critical_row,
            &updating,
            &send_update,
            &send_preview,
            &gallery_sounds,
        );

        // Subscribe to sound-config
        {
            let store = entity_store.clone();
            let group_ref = group.clone();
            let enabled_ref = enabled_row;
            let low_ref = low_row.clone();
            let normal_ref = normal_row.clone();
            let critical_ref = critical_row.clone();
            let urn_ref = current_urn;
            let guard = updating.clone();
            let gallery_ref = gallery_sounds.clone();

            entity_store.subscribe_type(SOUND_CONFIG_ENTITY_TYPE, move || {
                let entities: Vec<(Urn, SoundConfigEntity)> =
                    store.get_entities_typed(SOUND_CONFIG_ENTITY_TYPE);

                if let Some((urn, cfg)) = entities.first() {
                    guard.set(true);
                    *urn_ref.borrow_mut() = Some(urn.clone());
                    group_ref.set_visible(true);
                    enabled_ref.set_active(cfg.enabled);
                    set_urgency_value(&low_ref, &cfg.default_low, &gallery_ref.borrow());
                    set_urgency_value(&normal_ref, &cfg.default_normal, &gallery_ref.borrow());
                    set_urgency_value(&critical_ref, &cfg.default_critical, &gallery_ref.borrow());
                    guard.set(false);
                } else {
                    group_ref.set_visible(false);
                }
            });
        }

        // Subscribe to gallery sounds (to populate dropdowns)
        {
            let store = entity_store.clone();
            let gallery_ref = gallery_sounds;
            let low_ref = low_row;
            let normal_ref = normal_row;
            let critical_ref = critical_row;
            let guard = updating;

            entity_store.subscribe_type(NOTIFICATION_SOUND_ENTITY_TYPE, move || {
                let entities: Vec<(Urn, NotificationSound)> =
                    store.get_entities_typed(NOTIFICATION_SOUND_ENTITY_TYPE);
                let sounds: Vec<NotificationSound> =
                    entities.into_iter().map(|(_, s)| s).collect();

                guard.set(true);

                // Save current selections
                let low_val = resolve_urgency_value(&low_ref, &gallery_ref.borrow());
                let normal_val = resolve_urgency_value(&normal_ref, &gallery_ref.borrow());
                let critical_val = resolve_urgency_value(&critical_ref, &gallery_ref.borrow());

                *gallery_ref.borrow_mut() = sounds.clone();

                // Rebuild dropdown models
                rebuild_urgency_model(&low_ref, &sounds);
                rebuild_urgency_model(&normal_ref, &sounds);
                rebuild_urgency_model(&critical_ref, &sounds);

                // Restore selections
                set_urgency_value(&low_ref, &low_val, &sounds);
                set_urgency_value(&normal_ref, &normal_val, &sounds);
                set_urgency_value(&critical_ref, &critical_val, &sounds);

                guard.set(false);
            });
        }

        Self { root: group }
    }

    fn create_urgency_row(title: &str) -> UrgencyRow {
        let model = gtk::StringList::new(&[]);

        // Populate with XDG names
        for label in xdg_sound_labels() {
            model.append(&label);
        }
        // "Custom..." entry at the end
        model.append(&t("sounds-custom"));

        let combo = adw::ComboRow::builder()
            .title(title)
            .model(&model)
            .build();

        // Custom entry (hidden by default)
        let custom_entry = adw::EntryRow::builder()
            .title(t("sounds-custom-name"))
            .show_apply_button(true)
            .visible(false)
            .build();

        // Preview button as suffix
        let preview_btn = gtk::Button::builder()
            .icon_name("media-playback-start-symbolic")
            .css_classes(["flat"])
            .valign(gtk::Align::Center)
            .tooltip_text(t("sounds-preview"))
            .build();
        combo.add_suffix(&preview_btn);

        UrgencyRow {
            combo,
            custom_entry,
            preview_btn,
            model,
        }
    }

    fn wire_urgency_combo(
        urgency_row: &UrgencyRow,
        updating: &Rc<Cell<bool>>,
        send_update: &Rc<dyn Fn()>,
        send_preview: &Rc<dyn Fn(String)>,
        gallery_sounds: &Rc<RefCell<Vec<NotificationSound>>>,
    ) {
        // Show/hide custom entry based on selection
        {
            let guard = updating.clone();
            let send = send_update.clone();
            let combo = urgency_row.combo.clone();
            let combo_inner = combo.clone();
            let model = urgency_row.model.clone();
            let custom_entry = urgency_row.custom_entry.clone();

            combo.connect_selected_notify(move |_| {
                if guard.get() {
                    return;
                }
                let idx = combo_inner.selected();
                let n_items = model.n_items();
                if idx == n_items - 1 {
                    // "Custom..." selected
                    custom_entry.set_visible(true);
                } else {
                    custom_entry.set_visible(false);
                    send();
                }
            });
        }

        // Wire custom entry apply
        {
            let guard = updating.clone();
            let send = send_update.clone();
            urgency_row.custom_entry.connect_apply(move |_| {
                if guard.get() {
                    return;
                }
                send();
            });
        }

        // Preview button
        {
            let combo = urgency_row.combo.clone();
            let custom_entry = urgency_row.custom_entry.clone();
            let gallery_ref = gallery_sounds.clone();
            let model = urgency_row.model.clone();
            let preview = send_preview.clone();
            urgency_row.preview_btn.connect_clicked(move |_| {
                let row = UrgencyRow {
                    combo: combo.clone(),
                    custom_entry: custom_entry.clone(),
                    preview_btn: gtk::Button::new(), // dummy, not used
                    model: model.clone(),
                };
                let value = resolve_urgency_value(&row, &gallery_ref.borrow());
                if !value.is_empty() {
                    preview(value);
                }
            });
        }
    }
}

/// An urgency row with combo, custom entry, preview button, and string model.
#[derive(Clone)]
struct UrgencyRow {
    combo: adw::ComboRow,
    custom_entry: adw::EntryRow,
    preview_btn: gtk::Button,
    model: gtk::StringList,
}

/// Get the number of gallery items in the model.
///
/// Model layout: [gallery_sounds...] + [XDG_SOUNDS...] + ["Custom..."]
fn gallery_count(model: &gtk::StringList) -> u32 {
    let total = model.n_items();
    // XDG_SOUND_IDS count + 1 for "Custom..."
    let fixed = XDG_SOUND_IDS.len() as u32 + 1;
    total.saturating_sub(fixed)
}

/// Resolve the current value from a urgency row's selected index.
fn resolve_urgency_value(row: &UrgencyRow, gallery: &[NotificationSound]) -> String {
    let idx = row.combo.selected();
    let n_gallery = gallery_count(&row.model);
    let n_items = row.model.n_items();

    if idx == n_items - 1 {
        // "Custom..." -> use the entry text
        return row.custom_entry.text().to_string();
    }

    if idx < n_gallery {
        // Gallery sound
        if let Some(sound) = gallery.get(idx as usize) {
            return sound.reference.clone();
        }
    }

    // XDG sound
    let xdg_idx = (idx - n_gallery) as usize;
    if let Some(name) = XDG_SOUND_IDS.get(xdg_idx) {
        return name.to_string();
    }

    String::new()
}

/// Set a urgency row's selection to match a value.
fn set_urgency_value(row: &UrgencyRow, value: &str, gallery: &[NotificationSound]) {
    let n_gallery = gallery_count(&row.model);
    let n_items = row.model.n_items();

    // Check gallery sounds first
    for (i, sound) in gallery.iter().enumerate() {
        if sound.reference == value {
            row.combo.set_selected(i as u32);
            row.custom_entry.set_visible(false);
            return;
        }
    }

    // Check XDG sounds
    for (i, name) in XDG_SOUND_IDS.iter().enumerate() {
        if *name == value {
            row.combo.set_selected(n_gallery + i as u32);
            row.custom_entry.set_visible(false);
            return;
        }
    }

    // Custom value
    if !value.is_empty() {
        row.combo.set_selected(n_items - 1); // "Custom..."
        row.custom_entry.set_text(value);
        row.custom_entry.set_visible(true);
    } else {
        // Empty: select first XDG if available
        if n_items > 1 {
            row.combo.set_selected(n_gallery);
        }
        row.custom_entry.set_visible(false);
    }
}

/// Rebuild the dropdown model with current gallery sounds + XDG names + Custom.
fn rebuild_urgency_model(row: &UrgencyRow, gallery: &[NotificationSound]) {
    // Clear model
    let model = &row.model;
    while model.n_items() > 0 {
        model.remove(0);
    }

    // Gallery sounds
    for sound in gallery {
        model.append(&sound.filename);
    }

    // XDG sounds
    for label in xdg_sound_labels() {
        model.append(&label);
    }

    // Custom
    model.append(&t("sounds-custom"));
}
