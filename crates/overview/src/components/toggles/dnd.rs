//! Do Not Disturb toggle component.
//!
//! Subscribes to the `dnd` entity type and renders a FeatureToggleWidget
//! that silences notification toasts. Also switches icon when active.
//! Hidden until entity data arrives from the daemon.

use std::rc::Rc;

use waft_protocol::{Urn, entity};
use waft_client::{EntityActionCallback, EntityStore};

use crate::ui::feature_toggles::simple_toggle::{SimpleToggle, SimpleToggleConfig, ToggleUpdate};

pub fn dnd_toggle(
    store: &Rc<EntityStore>,
    action_callback: &EntityActionCallback,
    rebuild_callback: Rc<dyn Fn()>,
) -> SimpleToggle {
    SimpleToggle::new(
        store,
        action_callback,
        rebuild_callback,
        SimpleToggleConfig {
            entity_type: entity::notification::DND_ENTITY_TYPE,
            urn: Urn::new("notifications", "dnd", "default"),
            icon: "preferences-system-notifications-symbolic",
            title: crate::i18n::t("dnd-title"),
            widget_id: "dnd-toggle",
            weight: 60,
            on_update: |d: &entity::notification::Dnd| ToggleUpdate {
                active: d.active,
                details: d.active.then(|| crate::i18n::t("dnd-silenced")),
                icon: Some(if d.active {
                    "notifications-disabled-symbolic"
                } else {
                    "preferences-system-notifications-symbolic"
                }),
            },
        },
    )
}
