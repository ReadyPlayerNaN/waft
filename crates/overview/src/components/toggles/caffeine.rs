//! Caffeine (sleep inhibitor) toggle component.
//!
//! Subscribes to the `sleep-inhibitor` entity type from the caffeine plugin
//! and renders a FeatureToggleWidget that prevents the screen from sleeping.

use std::rc::Rc;

use waft_protocol::entity;
use waft_protocol::Urn;
use waft_ui_gtk::widgets::feature_toggle::{FeatureToggleProps, FeatureToggleWidget};

use crate::entity_store::{EntityActionCallback, EntityStore};
use crate::plugin::WidgetFeatureToggle;

/// Toggle for enabling/disabling screen sleep inhibition (caffeine mode).
pub struct CaffeineToggle {
    toggle: Rc<FeatureToggleWidget>,
}

impl CaffeineToggle {
    pub fn new(store: &Rc<EntityStore>, action_callback: &EntityActionCallback) -> Self {
        let toggle = Rc::new(FeatureToggleWidget::new(
            FeatureToggleProps {
                active: false,
                busy: false,
                details: None,
                expandable: false,
                icon: "preferences-system-power-symbolic".to_string(),
                title: "Caffeine".to_string(),
                menu_id: None,
            },
            None,
        ));

        // Connect output to route toggle actions to the daemon
        let cb = action_callback.clone();
        toggle.connect_output(move |_output| {
            let urn = Urn::new("caffeine", "sleep-inhibitor", "default");
            cb(urn, "toggle".to_string(), serde_json::Value::Null);
        });

        // Subscribe to entity changes and update the widget
        let store_ref = store.clone();
        let toggle_ref = toggle.clone();
        store.subscribe_type(entity::session::SLEEP_INHIBITOR_ENTITY_TYPE, move || {
            let entities: Vec<(Urn, entity::session::SleepInhibitor)> =
                store_ref.get_entities_typed(entity::session::SLEEP_INHIBITOR_ENTITY_TYPE);

            if let Some((_urn, inhibitor)) = entities.first() {
                toggle_ref.set_active(inhibitor.active);
                toggle_ref.set_details(if inhibitor.active {
                    Some("Screen will stay on".to_string())
                } else {
                    None
                });
            }
        });

        Self { toggle }
    }

    pub fn as_feature_toggle(&self) -> Rc<WidgetFeatureToggle> {
        Rc::new(WidgetFeatureToggle {
            id: "caffeine-toggle".to_string(),
            weight: 300,
            el: self.toggle.widget(),
            menu: None,
            on_expand_toggled: None,
            menu_id: None,
        })
    }
}
