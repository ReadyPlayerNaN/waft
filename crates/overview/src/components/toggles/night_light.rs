//! Night light toggle component.
//!
//! Subscribes to the `night-light` entity type from the sunsetr plugin and
//! renders a FeatureToggleWidget that enables/disables blue light filtering.

use std::rc::Rc;

use waft_protocol::entity;
use waft_protocol::Urn;
use waft_ui_gtk::widgets::feature_toggle::{FeatureToggleProps, FeatureToggleWidget};

use crate::entity_store::{EntityActionCallback, EntityStore};
use crate::plugin::WidgetFeatureToggle;

/// Toggle for enabling/disabling night light (blue light filter).
pub struct NightLightToggle {
    toggle: Rc<FeatureToggleWidget>,
}

impl NightLightToggle {
    pub fn new(store: &Rc<EntityStore>, action_callback: &EntityActionCallback) -> Self {
        let toggle = Rc::new(FeatureToggleWidget::new(
            FeatureToggleProps {
                active: false,
                busy: false,
                details: None,
                expandable: false,
                icon: "night-light-symbolic".to_string(),
                title: "Night Light".to_string(),
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
        store.subscribe_type(entity::display::NIGHT_LIGHT_ENTITY_TYPE, move || {
            let entities: Vec<(Urn, entity::display::NightLight)> =
                store_ref.get_entities_typed(entity::display::NIGHT_LIGHT_ENTITY_TYPE);

            if let Some((_urn, night_light)) = entities.first() {
                toggle_ref.set_active(night_light.active);
                toggle_ref.set_details(night_light.period.clone());
            }
        });

        Self { toggle }
    }

    pub fn as_feature_toggle(&self) -> Rc<WidgetFeatureToggle> {
        Rc::new(WidgetFeatureToggle {
            id: "night-light-toggle".to_string(),
            weight: 210,
            el: self.toggle.widget(),
            menu: None,
            on_expand_toggled: None,
            menu_id: None,
        })
    }
}
