//! Do Not Disturb toggle component.
//!
//! Subscribes to the `dnd` entity type from the notifications plugin and
//! renders a FeatureToggleWidget that silences notification toasts.
//! Hidden until entity data arrives from the daemon.

use std::cell::Cell;
use std::rc::Rc;

use waft_protocol::entity;
use waft_protocol::Urn;
use waft_ui_gtk::widgets::feature_toggle::{FeatureToggleProps, FeatureToggleWidget};

use crate::entity_store::{EntityActionCallback, EntityStore};
use crate::plugin::WidgetFeatureToggle;

/// Toggle for enabling/disabling Do Not Disturb mode.
///
/// Reports zero toggles until the first entity arrives, then one toggle.
pub struct DoNotDisturbToggle {
    toggle: Rc<FeatureToggleWidget>,
    available: Rc<Cell<bool>>,
}

impl DoNotDisturbToggle {
    pub fn new(
        store: &Rc<EntityStore>,
        action_callback: &EntityActionCallback,
        rebuild_callback: Rc<dyn Fn()>,
    ) -> Self {
        let available = Rc::new(Cell::new(false));

        let toggle = Rc::new(FeatureToggleWidget::new(
            FeatureToggleProps {
                active: false,
                busy: false,
                details: None,
                expandable: false,
                icon: "preferences-system-notifications-symbolic".to_string(),
                title: crate::i18n::t("dnd-title"),
                menu_id: None,
            },
            None,
        ));

        // Connect output to route toggle actions to the daemon
        let cb = action_callback.clone();
        toggle.connect_output(move |_output| {
            let urn = Urn::new("notifications", "dnd", "default");
            cb(urn, "toggle".to_string(), serde_json::Value::Null);
        });

        // Subscribe to entity changes and update the widget
        let store_ref = store.clone();
        let toggle_ref = toggle.clone();
        let available_ref = available.clone();
        store.subscribe_type(entity::notification::DND_ENTITY_TYPE, move || {
            let entities: Vec<(Urn, entity::notification::Dnd)> =
                store_ref.get_entities_typed(entity::notification::DND_ENTITY_TYPE);

            let was_available = available_ref.get();
            let now_available = !entities.is_empty();

            if let Some((_urn, dnd)) = entities.first() {
                toggle_ref.set_active(dnd.active);
                toggle_ref.set_icon(if dnd.active {
                    "notifications-disabled-symbolic"
                } else {
                    "preferences-system-notifications-symbolic"
                });
                toggle_ref.set_details(if dnd.active {
                    Some(crate::i18n::t("dnd-silenced"))
                } else {
                    None
                });
            }

            if was_available != now_available {
                available_ref.set(now_available);
                rebuild_callback();
            }
        });

        Self { toggle, available }
    }

    pub fn as_feature_toggles(&self) -> Vec<Rc<WidgetFeatureToggle>> {
        if !self.available.get() {
            return Vec::new();
        }
        vec![Rc::new(WidgetFeatureToggle {
            id: "dnd-toggle".to_string(),
            weight: 60,
            toggle: (*self.toggle).clone(),
            menu: None,
        })]
    }
}
