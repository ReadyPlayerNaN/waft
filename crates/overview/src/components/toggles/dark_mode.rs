//! Dark mode toggle component.
//!
//! Subscribes to the `dark-mode` entity type and renders a FeatureToggleWidget
//! that switches between light and dark themes.
//! Hidden until entity data arrives from the daemon.

use std::rc::Rc;

use waft_protocol::{Urn, entity};
use waft_client::{EntityActionCallback, EntityStore};

use crate::ui::feature_toggles::simple_toggle::{SimpleToggle, SimpleToggleConfig, ToggleUpdate};

pub fn dark_mode_toggle(
    store: &Rc<EntityStore>,
    action_callback: &EntityActionCallback,
    rebuild_callback: Rc<dyn Fn()>,
) -> SimpleToggle {
    SimpleToggle::new(
        store,
        action_callback,
        rebuild_callback,
        SimpleToggleConfig {
            entity_type: entity::display::DARK_MODE_ENTITY_TYPE,
            urn: Urn::new("darkman", "dark-mode", "default"),
            icon: "weather-clear-night-symbolic",
            title: crate::i18n::t("darkman-title"),
            widget_id: "dark-mode-toggle",
            weight: 200,
            on_update: |d: &entity::display::DarkMode| ToggleUpdate {
                active: d.active,
                details: None,
                icon: None,
            },
        },
    )
}
