//! Wallpaper settings page -- smart container.
//!
//! Subscribes to `EntityStore` for `wallpaper-manager` entity type.
//! Composes mode, preview, transition, and configuration sections.
//! Routes entity data down and user actions up.

use std::cell::RefCell;
use std::rc::Rc;

use gtk::prelude::*;
use waft_client::{EntityActionCallback, EntityStore};
use waft_protocol::Urn;
use waft_protocol::entity::display::{
    WallpaperManager, WallpaperMode, WALLPAPER_MANAGER_ENTITY_TYPE,
};

use crate::i18n::t;
use crate::kdl_config;
use crate::search_index::SearchIndex;
use crate::wallpaper::background_color_section::BackgroundColorSection;
use crate::wallpaper::config_section::{ConfigSection, ConfigSectionOutput};
use crate::wallpaper::gallery_section::GallerySection;
use crate::wallpaper::mode_section::{ModeSection, ModeSectionOutput};
use crate::wallpaper::preview_section::{PreviewSection, PreviewSectionOutput};
use crate::wallpaper::transition_section::{TransitionSection, TransitionSectionOutput};

/// Smart container for the Wallpaper settings page.
pub struct WallpaperPage {
    pub root: gtk::Box,
}

impl WallpaperPage {
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

        let mode = Rc::new(ModeSection::new());
        root.append(&mode.root);

        let preview = Rc::new(PreviewSection::new());
        root.append(&preview.root);

        let transition = Rc::new(TransitionSection::new());
        root.append(&transition.root);

        let config = Rc::new(ConfigSection::new());
        root.append(&config.root);

        let bg_color_path = kdl_config::niri_config_path();
        let bg_color = Rc::new(BackgroundColorSection::new(&bg_color_path));
        root.append(&bg_color.root);

        let gallery = Rc::new(GallerySection::new(action_callback));
        root.append(&gallery.root);

        // Register search entries
        {
            let mut idx = search_index.borrow_mut();
            let page_title = t("settings-wallpaper");
            idx.add_section("wallpaper", &page_title, &t("wallpaper-mode"), "wallpaper-mode", &mode.root);
            idx.add_section("wallpaper", &page_title, &t("wallpaper-current"), "wallpaper-current", &preview.root);
            idx.add_section("wallpaper", &page_title, &t("wallpaper-transition"), "wallpaper-transition", &transition.root);
            idx.add_section("wallpaper", &page_title, &t("wallpaper-config"), "wallpaper-config", &config.root);
            idx.add_section("wallpaper", &page_title, &t("wallpaper-background-color"), "wallpaper-background-color", &bg_color.root);
        }

        // Current URN for the "all" entity (or first output)
        let current_urn: Rc<RefCell<Option<Urn>>> = Rc::new(RefCell::new(None));

        // Wire mode output
        {
            let cb = action_callback.clone();
            let urn_ref = current_urn.clone();
            let preview_ref = preview.clone();
            mode.connect_output(move |output| {
                if let Some(ref urn) = *urn_ref.borrow() {
                    let ModeSectionOutput::ModeChanged { mode } = output;
                    preview_ref.set_browse_visible(mode == "static");
                    cb(
                        urn.clone(),
                        "set-mode".to_string(),
                        serde_json::json!({ "mode": mode }),
                    );
                }
            });
        }

        // Wire preview output
        {
            let cb = action_callback.clone();
            let urn_ref = current_urn.clone();
            preview.connect_output(move |output| {
                if let Some(ref urn) = *urn_ref.borrow() {
                    match output {
                        PreviewSectionOutput::SetWallpaper(path) => {
                            cb(
                                urn.clone(),
                                "set-wallpaper".to_string(),
                                serde_json::json!({ "path": path }),
                            );
                        }
                        PreviewSectionOutput::Random => {
                            cb(
                                urn.clone(),
                                "random".to_string(),
                                serde_json::json!({}),
                            );
                        }
                    }
                }
            });
        }

        // Wire transition output
        {
            let cb = action_callback.clone();
            let urn_ref = current_urn.clone();
            transition.connect_output(move |output| {
                if let Some(ref urn) = *urn_ref.borrow() {
                    let TransitionSectionOutput::TransitionChanged {
                        transition_type,
                        fps,
                        angle,
                        duration,
                    } = output;
                    cb(
                        urn.clone(),
                        "update-transition".to_string(),
                        serde_json::json!({
                            "transition_type": transition_type,
                            "fps": fps,
                            "angle": angle,
                            "duration": duration,
                        }),
                    );
                }
            });
        }

        // Wire config output
        {
            let cb = action_callback.clone();
            let urn_ref = current_urn.clone();
            config.connect_output(move |output| {
                if let Some(ref urn) = *urn_ref.borrow() {
                    let ConfigSectionOutput::ConfigChanged { wallpaper_dir, sync } = output;
                    cb(
                        urn.clone(),
                        "update-config".to_string(),
                        serde_json::json!({
                            "wallpaper_dir": wallpaper_dir,
                            "sync": sync,
                        }),
                    );
                }
            });
        }

        // Reconciliation helper
        fn reconcile(
            entities: &[(Urn, WallpaperManager)],
            urn_ref: &Rc<RefCell<Option<Urn>>>,
            mode: &Rc<ModeSection>,
            preview: &Rc<PreviewSection>,
            transition: &Rc<TransitionSection>,
            config: &Rc<ConfigSection>,
            gallery: &Rc<GallerySection>,
        ) {
            // Prefer the "all" entity for display
            let target = entities
                .iter()
                .find(|(_, m)| m.output == "all")
                .or_else(|| entities.first());

            if let Some((urn, manager)) = target {
                *urn_ref.borrow_mut() = Some(urn.clone());

                mode.apply_props(
                    &manager.mode,
                    manager.current_segment.as_ref(),
                    manager.style_tracking_available,
                );
                preview.apply_props(
                    manager.current_wallpaper.as_deref(),
                    manager.available,
                );
                preview.set_browse_visible(matches!(manager.mode, WallpaperMode::Static));
                transition.apply_props(
                    &manager.transition.transition_type,
                    manager.transition.fps,
                    manager.transition.angle,
                    manager.transition.duration,
                );
                transition.set_sensitive(manager.available);
                config.apply_props(&manager.wallpaper_dir, manager.sync, &manager.mode);
                config.set_sensitive(manager.available);
                gallery.apply_props(
                    &manager.wallpaper_dir,
                    &manager.mode,
                    manager.current_wallpaper.as_deref(),
                    urn,
                );
            }
        }

        // Subscribe to wallpaper-manager entities
        {
            let store = entity_store.clone();
            let urn_ref = current_urn.clone();
            let mode_ref = mode.clone();
            let preview_ref = preview.clone();
            let transition_ref = transition.clone();
            let config_ref = config.clone();
            let gallery_ref = gallery.clone();

            entity_store.subscribe_type(WALLPAPER_MANAGER_ENTITY_TYPE, move || {
                let entities: Vec<(Urn, WallpaperManager)> =
                    store.get_entities_typed(WALLPAPER_MANAGER_ENTITY_TYPE);
                reconcile(&entities, &urn_ref, &mode_ref, &preview_ref, &transition_ref, &config_ref, &gallery_ref);
            });
        }

        // Initial reconciliation with cached data
        {
            let store = entity_store.clone();
            let urn_ref = current_urn;
            let mode_ref = mode.clone();
            let preview_ref = preview.clone();
            let transition_ref = transition.clone();
            let config_ref = config.clone();
            let gallery_ref = gallery.clone();

            gtk::glib::idle_add_local_once(move || {
                let entities: Vec<(Urn, WallpaperManager)> =
                    store.get_entities_typed(WALLPAPER_MANAGER_ENTITY_TYPE);
                if !entities.is_empty() {
                    log::debug!(
                        "[wallpaper-page] Initial reconciliation: {} entities",
                        entities.len()
                    );
                    reconcile(&entities, &urn_ref, &mode_ref, &preview_ref, &transition_ref, &config_ref, &gallery_ref);
                }
            });
        }

        // Prevent sections from being dropped
        std::mem::forget(mode);
        std::mem::forget(preview);
        std::mem::forget(transition);
        std::mem::forget(config);
        std::mem::forget(bg_color);
        std::mem::forget(gallery);

        Self { root }
    }
}
