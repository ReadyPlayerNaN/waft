//! Dark mode toggle component.
//!
//! Subscribes to the `dark-mode` entity type from the darkman plugin and
//! renders a FeatureToggleWidget that switches between light and dark themes.
//! Hidden until entity data arrives from the daemon.

use std::cell::Cell;
use std::rc::Rc;

use waft_protocol::entity;
use waft_protocol::Urn;
use waft_ui_gtk::widgets::feature_toggle::{FeatureToggleProps, FeatureToggleWidget};

use crate::entity_store::{EntityActionCallback, EntityStore};
use crate::plugin::WidgetFeatureToggle;

/// Toggle for enabling/disabling dark mode via the darkman daemon.
///
/// Reports zero toggles until the first entity arrives, then one toggle.
pub struct DarkModeToggle {
    toggle: Rc<FeatureToggleWidget>,
    available: Rc<Cell<bool>>,
}

impl DarkModeToggle {
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
                icon: "weather-clear-night-symbolic".to_string(),
                title: "Dark Mode".to_string(),
                menu_id: None,
            },
            None,
        ));

        // Connect output to route toggle actions to the daemon
        let cb = action_callback.clone();
        toggle.connect_output(move |_output| {
            let urn = Urn::new("darkman", "dark-mode", "default");
            cb(urn, "toggle".to_string(), serde_json::Value::Null);
        });

        // Subscribe to entity changes and update the widget
        let store_ref = store.clone();
        let toggle_ref = toggle.clone();
        let available_ref = available.clone();
        store.subscribe_type(entity::display::DARK_MODE_ENTITY_TYPE, move || {
            let entities: Vec<(Urn, entity::display::DarkMode)> =
                store_ref.get_entities_typed(entity::display::DARK_MODE_ENTITY_TYPE);

            let was_available = available_ref.get();
            let now_available = !entities.is_empty();

            if let Some((_urn, dark_mode)) = entities.first() {
                toggle_ref.set_active(dark_mode.active);
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
            id: "dark-mode-toggle".to_string(),
            weight: 200,
            el: self.toggle.widget(),
            menu: None,
            on_expand_toggled: None,
            menu_id: None,
        })]
    }
}
