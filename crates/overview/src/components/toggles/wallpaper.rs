//! Wallpaper service toggle component.
//!
//! Subscribes to the `wallpaper-manager` entity type and renders a FeatureToggleWidget
//! that starts/stops the wallpaper backend daemon.

use std::rc::Rc;

use waft_client::{EntityActionCallback, EntityStore};
use waft_protocol::{Urn, entity};

use crate::ui::feature_toggles::simple_toggle::{SimpleToggle, SimpleToggleConfig, ToggleUpdate};

pub fn wallpaper_toggle(
    store: &Rc<EntityStore>,
    action_callback: &EntityActionCallback,
    rebuild_callback: Rc<dyn Fn()>,
) -> SimpleToggle {
    SimpleToggle::new(
        store,
        action_callback,
        rebuild_callback,
        SimpleToggleConfig {
            entity_type: entity::display::WALLPAPER_MANAGER_ENTITY_TYPE,
            urn: Urn::new("awww", "wallpaper-manager", "all"),
            icon: "preferences-desktop-wallpaper-symbolic",
            title: crate::i18n::t("wallpaper-title"),
            widget_id: "wallpaper-toggle",
            weight: 220,
            on_update: |w: &entity::display::WallpaperManager| ToggleUpdate {
                active: w.active,
                details: Some(if w.active {
                    crate::i18n::t("wallpaper-active")
                } else {
                    crate::i18n::t("wallpaper-inactive")
                }),
                icon: None,
            },
            action_for_click: |_w, currently_active| {
                if currently_active { "stop" } else { "start" }
            },
        },
    )
}
