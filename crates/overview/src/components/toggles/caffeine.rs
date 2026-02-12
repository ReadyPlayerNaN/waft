//! Caffeine (sleep inhibitor) toggle component.
//!
//! Subscribes to the `sleep-inhibitor` entity type from the caffeine plugin
//! and renders a FeatureToggleWidget that prevents the screen from sleeping.
//! Hidden until entity data arrives from the daemon.

use std::cell::Cell;
use std::rc::Rc;

use waft_protocol::entity;
use waft_protocol::Urn;
use waft_ui_gtk::widgets::feature_toggle::{FeatureToggleProps, FeatureToggleWidget};

use crate::entity_store::{EntityActionCallback, EntityStore};
use crate::plugin::WidgetFeatureToggle;

/// Toggle for enabling/disabling screen sleep inhibition (caffeine mode).
///
/// Reports zero toggles until the first entity arrives, then one toggle.
pub struct CaffeineToggle {
    toggle: Rc<FeatureToggleWidget>,
    available: Rc<Cell<bool>>,
}

impl CaffeineToggle {
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
        let available_ref = available.clone();
        store.subscribe_type(entity::session::SLEEP_INHIBITOR_ENTITY_TYPE, move || {
            let entities: Vec<(Urn, entity::session::SleepInhibitor)> =
                store_ref.get_entities_typed(entity::session::SLEEP_INHIBITOR_ENTITY_TYPE);

            let was_available = available_ref.get();
            let now_available = !entities.is_empty();

            if let Some((_urn, inhibitor)) = entities.first() {
                toggle_ref.set_active(inhibitor.active);
                toggle_ref.set_details(if inhibitor.active {
                    Some("Screen will stay on".to_string())
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
            id: "caffeine-toggle".to_string(),
            weight: 300,
            el: self.toggle.widget(),
            menu: None,
            on_expand_toggled: None,
            menu_id: None,
        })]
    }
}
