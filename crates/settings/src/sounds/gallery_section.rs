//! Sound gallery section -- smart container.
//!
//! Subscribes to `EntityStore` for `notification-sound` entity type.
//! Lists gallery sounds with preview and remove buttons.
//! Provides file upload via `gtk::FileDialog`.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use adw::prelude::*;
use waft_client::{EntityActionCallback, EntityStore};
use crate::search_index::SearchIndex;
use waft_protocol::Urn;
use waft_protocol::entity::notification_sound::{NOTIFICATION_SOUND_ENTITY_TYPE, NotificationSound};

use crate::i18n::t;

/// Smart container for the sound gallery.
pub struct GallerySection {
    pub root: adw::PreferencesGroup,
}

struct SoundRow {
    row: adw::ActionRow,
}

impl GallerySection {
    /// Phase 1: Register static search entries without constructing widgets.
    pub fn register_search(idx: &mut SearchIndex) {
        let page_title = t("settings-sounds");
        let section_title = t("sounds-gallery");
        idx.add_section_deferred("sounds", &page_title, &section_title, "sounds-gallery");
        idx.add_input_deferred("sounds", &page_title, &section_title, &t("sounds-add-file"), "sounds-add-file");
    }

    pub fn new(
        entity_store: &Rc<EntityStore>,
        action_callback: &EntityActionCallback,
        search_index: &Rc<RefCell<SearchIndex>>,
    ) -> Self {
        let group = adw::PreferencesGroup::builder()
            .title(t("sounds-gallery"))
            .description(t("sounds-gallery-desc"))
            .build();

        // Add button in header
        let add_button = gtk::Button::builder()
            .icon_name("list-add-symbolic")
            .css_classes(["flat"])
            .valign(gtk::Align::Center)
            .tooltip_text(t("sounds-add-file"))
            .build();
        group.set_header_suffix(Some(&add_button));

        // Register search entries
        {
            let mut idx = search_index.borrow_mut();
            let page_title = t("settings-sounds");
            let section_title = t("sounds-gallery");
            idx.add_section("sounds", &page_title, &section_title, "sounds-gallery", &group);
            idx.add_input("sounds", &page_title, &section_title, &t("sounds-add-file"), "sounds-add-file", &add_button);
        }

        // Wire add button to open file dialog
        {
            let cb = action_callback.clone();
            let group_ref = group.clone();
            add_button.connect_clicked(move |button| {
                let dialog = gtk::FileDialog::builder()
                    .title("Select Sound File")
                    .modal(true)
                    .build();

                // Audio file filter
                let filter = gtk::FileFilter::new();
                filter.set_name(Some("Audio files"));
                filter.add_mime_type("audio/ogg");
                filter.add_mime_type("audio/x-wav");
                filter.add_mime_type("audio/wav");
                filter.add_mime_type("audio/flac");
                filter.add_mime_type("audio/mpeg");
                filter.add_pattern("*.ogg");
                filter.add_pattern("*.oga");
                filter.add_pattern("*.wav");
                filter.add_pattern("*.flac");
                filter.add_pattern("*.mp3");

                let filters = gtk::gio::ListStore::new::<gtk::FileFilter>();
                filters.append(&filter);
                dialog.set_filters(Some(&filters));

                let window = button
                    .root()
                    .and_then(|r| r.downcast::<gtk::Window>().ok());

                let cb_inner = cb.clone();
                let group_inner = group_ref.clone();
                dialog.open(window.as_ref(), gtk::gio::Cancellable::NONE, move |result: Result<gtk::gio::File, gtk::glib::Error>| {
                    let file: gtk::gio::File = match result {
                        Ok(f) => f,
                        Err(e) => {
                            // User cancelled or error
                            if !e.matches(gtk::gio::IOErrorEnum::Cancelled) {
                                log::warn!("[sounds/gallery] file dialog error: {e}");
                            }
                            return;
                        }
                    };

                    let Some(path) = file.path() else {
                        log::warn!("[sounds/gallery] selected file has no path");
                        return;
                    };

                    let filename = path
                        .file_name()
                        .and_then(|n: &std::ffi::OsStr| n.to_str())
                        .unwrap_or("unknown.ogg")
                        .to_string();

                    // Read file
                    let data = match std::fs::read(&path) {
                        Ok(d) => d,
                        Err(e) => {
                            log::warn!("[sounds/gallery] failed to read file: {e}");
                            show_toast(&group_inner, &format!("Failed to read file: {e}"));
                            return;
                        }
                    };

                    // Check size limit (5 MB)
                    if data.len() > 5 * 1024 * 1024 {
                        show_toast(
                            &group_inner,
                            "File exceeds 5 MB size limit",
                        );
                        return;
                    }

                    // Base64 encode
                    use base64::Engine;
                    let encoded = base64::engine::general_purpose::STANDARD.encode(&data);

                    // Send add-sound action
                    let urn = Urn::new("notifications", NOTIFICATION_SOUND_ENTITY_TYPE, &filename);
                    cb_inner(
                        urn,
                        "add-sound".to_string(),
                        serde_json::json!({
                            "filename": filename,
                            "data": encoded,
                        }),
                    );
                });
            });
        }

        let rows: Rc<RefCell<HashMap<String, SoundRow>>> = Rc::new(RefCell::new(HashMap::new()));

        // Subscribe to notification-sound entities
        {
            let store = entity_store.clone();
            let group_ref = group.clone();
            let rows_ref = rows.clone();
            let cb = action_callback.clone();

            entity_store.subscribe_type(NOTIFICATION_SOUND_ENTITY_TYPE, move || {
                let entities: Vec<(Urn, NotificationSound)> =
                    store.get_entities_typed(NOTIFICATION_SOUND_ENTITY_TYPE);
                Self::reconcile(&group_ref, &rows_ref, &entities, &cb);
            });
        }

        // Initial reconciliation
        {
            let store = entity_store.clone();
            let group_ref = group.clone();
            let rows_ref = rows;
            let cb = action_callback.clone();

            gtk::glib::idle_add_local_once(move || {
                let entities: Vec<(Urn, NotificationSound)> =
                    store.get_entities_typed(NOTIFICATION_SOUND_ENTITY_TYPE);
                if !entities.is_empty() {
                    log::debug!(
                        "[sounds/gallery] initial reconciliation: {} sounds",
                        entities.len()
                    );
                    Self::reconcile(&group_ref, &rows_ref, &entities, &cb);
                }
            });
        }

        Self { root: group }
    }

    fn reconcile(
        group: &adw::PreferencesGroup,
        rows: &Rc<RefCell<HashMap<String, SoundRow>>>,
        entities: &[(Urn, NotificationSound)],
        action_callback: &EntityActionCallback,
    ) {
        let mut rows_map = rows.borrow_mut();

        // Collect current entity filenames
        let entity_filenames: std::collections::HashSet<String> =
            entities.iter().map(|(_, s)| s.filename.clone()).collect();

        // Remove rows for sounds that no longer exist
        let to_remove: Vec<String> = rows_map
            .keys()
            .filter(|k| !entity_filenames.contains(k.as_str()))
            .cloned()
            .collect();
        for key in to_remove {
            if let Some(sound_row) = rows_map.remove(&key) {
                group.remove(&sound_row.row);
            }
        }

        // Add or update rows
        for (_urn, sound) in entities {
            if rows_map.contains_key(&sound.filename) {
                // Row already exists, update subtitle
                if let Some(existing) = rows_map.get(&sound.filename) {
                    existing.row.set_subtitle(&format_size(sound.size));
                }
                continue;
            }

            let row = adw::ActionRow::builder()
                .title(&sound.filename)
                .subtitle(format_size(sound.size))
                .build();

            // Preview button
            let preview_btn = gtk::Button::builder()
                .icon_name("media-playback-start-symbolic")
                .css_classes(["flat"])
                .valign(gtk::Align::Center)
                .tooltip_text(t("sounds-preview"))
                .build();

            {
                let cb = action_callback.clone();
                let reference = sound.reference.clone();
                preview_btn.connect_clicked(move |_| {
                    let urn = Urn::new(
                        "notifications",
                        "sound-config",
                        "default",
                    );
                    cb(
                        urn,
                        "preview-sound".to_string(),
                        serde_json::json!({ "reference": reference }),
                    );
                });
            }

            // Remove button
            let remove_btn = gtk::Button::builder()
                .icon_name("user-trash-symbolic")
                .css_classes(["flat"])
                .valign(gtk::Align::Center)
                .tooltip_text(t("sounds-remove"))
                .build();

            {
                let cb = action_callback.clone();
                let filename = sound.filename.clone();
                remove_btn.connect_clicked(move |_| {
                    let urn = Urn::new(
                        "notifications",
                        NOTIFICATION_SOUND_ENTITY_TYPE,
                        &filename,
                    );
                    cb(
                        urn,
                        "remove-sound".to_string(),
                        serde_json::Value::Null,
                    );
                });
            }

            row.add_suffix(&preview_btn);
            row.add_suffix(&remove_btn);
            group.add(&row);

            rows_map.insert(
                sound.filename.clone(),
                SoundRow { row },
            );
        }
    }
}

/// Format a file size in human-readable form.
fn format_size(bytes: u64) -> String {
    if bytes < 1024 {
        format!("{bytes} B")
    } else if bytes < 1024 * 1024 {
        format!("{:.1} KB", bytes as f64 / 1024.0)
    } else {
        format!("{:.1} MB", bytes as f64 / (1024.0 * 1024.0))
    }
}

/// Show a toast on the nearest window ancestor.
fn show_toast(widget: &impl IsA<gtk::Widget>, message: &str) {
    if let Some(window) = widget
        .root()
        .and_then(|r| r.downcast::<adw::ApplicationWindow>().ok())
    {
        // Try to find an AdwToastOverlay in the window's content hierarchy
        // Fallback: log the message since we may not have a toast overlay
        log::warn!("[sounds/gallery] toast: {message}");
        // Create a simple dialog as fallback
        let dialog = adw::AlertDialog::builder()
            .heading(t("sounds-error-heading"))
            .body(message)
            .build();
        dialog.add_response("ok", "OK");
        dialog.present(Some(&window));
    } else {
        log::warn!("[sounds/gallery] toast (no window): {message}");
    }
}
