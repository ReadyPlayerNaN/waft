//! Night light toggle component.
//!
//! Subscribes to the `night-light` entity type from the sunsetr plugin and
//! renders a FeatureToggleWidget that enables/disables blue light filtering.
//! Hidden until entity data arrives from the daemon.

use std::cell::Cell;
use std::rc::Rc;

use waft_protocol::entity;
use waft_protocol::Urn;
use waft_ui_gtk::widgets::feature_toggle::{FeatureToggleProps, FeatureToggleWidget};

use crate::entity_store::{EntityActionCallback, EntityStore};
use crate::layout::types::WidgetFeatureToggle;

/// Toggle for enabling/disabling night light (blue light filter).
///
/// Reports zero toggles until the first entity arrives, then one toggle.
pub struct NightLightToggle {
    toggle: Rc<FeatureToggleWidget>,
    available: Rc<Cell<bool>>,
}

impl NightLightToggle {
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
                icon: "night-light-symbolic".to_string(),
                title: crate::i18n::t("nightlight-title"),
                menu_id: None,
            },
            None,
        ));

        // Connect output to route toggle actions to the daemon
        let cb = action_callback.clone();
        toggle.connect_output(move |_output| {
            let urn = Urn::new("sunsetr", "night-light", "default");
            cb(urn, "toggle".to_string(), serde_json::Value::Null);
        });

        // Subscribe to entity changes and update the widget
        let store_ref = store.clone();
        let toggle_ref = toggle.clone();
        let available_ref = available.clone();
        store.subscribe_type(entity::display::NIGHT_LIGHT_ENTITY_TYPE, move || {
            let entities: Vec<(Urn, entity::display::NightLight)> =
                store_ref.get_entities_typed(entity::display::NIGHT_LIGHT_ENTITY_TYPE);

            let was_available = available_ref.get();
            let now_available = !entities.is_empty();

            if let Some((_urn, night_light)) = entities.first() {
                toggle_ref.set_active(night_light.active);
                toggle_ref.set_details(night_light.period.clone());
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
            id: "night-light-toggle".to_string(),
            weight: 210,
            toggle: (*self.toggle).clone(),
            menu: None,
        })]
    }
}
