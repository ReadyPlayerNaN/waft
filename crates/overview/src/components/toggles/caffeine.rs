//! Caffeine (sleep inhibitor) toggle component.
//!
//! Subscribes to the `sleep-inhibitor` entity type and renders a
//! FeatureToggleWidget that prevents the screen from sleeping.
//! Hidden until entity data arrives from the daemon.

use std::rc::Rc;

use waft_protocol::{Urn, entity};
use waft_client::{EntityActionCallback, EntityStore};

use crate::ui::feature_toggles::simple_toggle::{SimpleToggle, SimpleToggleConfig, ToggleUpdate};

pub fn caffeine_toggle(
    store: &Rc<EntityStore>,
    action_callback: &EntityActionCallback,
    rebuild_callback: Rc<dyn Fn()>,
) -> SimpleToggle {
    SimpleToggle::new(
        store,
        action_callback,
        rebuild_callback,
        SimpleToggleConfig {
            entity_type: entity::session::SLEEP_INHIBITOR_ENTITY_TYPE,
            urn: Urn::new("caffeine", "sleep-inhibitor", "default"),
            icon: "changes-allow-symbolic",
            title: crate::i18n::t("caffeine-title"),
            widget_id: "caffeine-toggle",
            weight: 300,
            on_update: |i: &entity::session::SleepInhibitor| ToggleUpdate {
                active: i.active,
                details: i.active.then(|| crate::i18n::t("caffeine-active")),
                icon: None,
            },
        },
    )
}
