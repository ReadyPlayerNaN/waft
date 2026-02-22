//! Night light toggle component.
//!
//! Subscribes to the `night-light` entity type and renders a FeatureToggleWidget
//! that enables/disables blue light filtering. Shows the current period as detail text.
//! Hidden until entity data arrives from the daemon.

use std::rc::Rc;

use waft_protocol::{Urn, entity};
use waft_client::{EntityActionCallback, EntityStore};

use crate::ui::feature_toggles::simple_toggle::{SimpleToggle, SimpleToggleConfig, ToggleUpdate};

pub fn night_light_toggle(
    store: &Rc<EntityStore>,
    action_callback: &EntityActionCallback,
    rebuild_callback: Rc<dyn Fn()>,
) -> SimpleToggle {
    SimpleToggle::new(
        store,
        action_callback,
        rebuild_callback,
        SimpleToggleConfig {
            entity_type: entity::display::NIGHT_LIGHT_ENTITY_TYPE,
            urn: Urn::new("sunsetr", "night-light", "default"),
            icon: "night-light-symbolic",
            title: crate::i18n::t("nightlight-title"),
            widget_id: "night-light-toggle",
            weight: 210,
            on_update: |n: &entity::display::NightLight| ToggleUpdate {
                active: n.active,
                details: n.period.clone(),
                icon: None,
            },
        },
    )
}
