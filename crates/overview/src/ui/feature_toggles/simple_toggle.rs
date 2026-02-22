//! Generic single-entity feature toggle.
//!
//! `SimpleToggle` covers the common case of a feature toggle backed by
//! exactly one entity type, a "toggle" action, and no expandable menu.
//! Hidden until entity data arrives from the daemon.

use std::cell::Cell;
use std::rc::Rc;

use waft_protocol::Urn;
use waft_ui_gtk::widgets::feature_toggle::{FeatureToggleProps, FeatureToggleWidget};

use crate::layout::types::WidgetFeatureToggle;
use waft_client::{EntityActionCallback, EntityStore};

/// Widget state update derived from a received entity.
pub struct ToggleUpdate {
    pub active: bool,
    /// None keeps existing details text; Some(_) replaces it.
    pub details: Option<String>,
    /// None keeps the current icon; Some("name") replaces it.
    /// Must be a `'static` string literal; runtime-computed icon names are not supported.
    pub icon: Option<&'static str>,
}

/// Configuration for a `SimpleToggle`.
pub struct SimpleToggleConfig<E> {
    /// Entity type constant (e.g. `entity::session::SLEEP_INHIBITOR_ENTITY_TYPE`).
    pub entity_type: &'static str,
    /// URN to dispatch the "toggle" action to.
    pub urn: Urn,
    /// Initial icon name (from the icon theme).
    pub icon: &'static str,
    /// Localized display title.
    pub title: String,
    /// Stable widget ID used in `WidgetFeatureToggle`.
    pub widget_id: &'static str,
    /// Sort weight in the feature grid (lower = further left).
    pub weight: i32,
    /// Maps a received entity to widget state updates.
    pub on_update: fn(&E) -> ToggleUpdate,
}

/// A single-entity, no-menu feature toggle.
///
/// Reports zero toggles until the first entity arrives from the daemon,
/// then exactly one. Dispatches `"toggle"` action on click.
pub struct SimpleToggle {
    toggle: Rc<FeatureToggleWidget>,
    available: Rc<Cell<bool>>,
    widget_id: &'static str,
    weight: i32,
}

impl SimpleToggle {
    pub fn new<E>(
        store: &Rc<EntityStore>,
        action_callback: &EntityActionCallback,
        rebuild_callback: Rc<dyn Fn()>,
        config: SimpleToggleConfig<E>,
    ) -> Self
    where
        E: serde::de::DeserializeOwned + 'static,
    {
        let available = Rc::new(Cell::new(false));

        let toggle = Rc::new(FeatureToggleWidget::new(
            FeatureToggleProps {
                active: false,
                busy: false,
                details: None,
                expandable: false,
                icon: config.icon.to_string(),
                title: config.title,
                menu_id: None,
            },
            None,
        ));

        let cb = action_callback.clone();
        let urn = config.urn;
        // SimpleToggle has exactly one output variant; all clicks dispatch "toggle".
        toggle.connect_output(move |_output| {
            cb(urn.clone(), "toggle".to_string(), serde_json::Value::Null);
        });

        let store_ref = store.clone();
        let toggle_ref = toggle.clone();
        let available_ref = available.clone();
        let entity_type = config.entity_type;
        let on_update = config.on_update;

        {
            let store_ref_sub = store_ref.clone();
            let toggle_ref_sub = toggle_ref.clone();
            let available_ref_sub = available_ref.clone();
            let rebuild_callback_sub = rebuild_callback.clone();
            store.subscribe_type(entity_type, move || {
                let entities: Vec<(Urn, E)> = store_ref_sub.get_entities_typed(entity_type);

                let was_available = available_ref_sub.get();
                let now_available = !entities.is_empty();

                if let Some((_urn, entity)) = entities.first() {
                    let update = on_update(entity);
                    toggle_ref_sub.set_active(update.active);
                    toggle_ref_sub.set_details(update.details);
                    if let Some(icon) = update.icon {
                        toggle_ref_sub.set_icon(icon);
                    }
                }

                if was_available != now_available {
                    available_ref_sub.set(now_available);
                    rebuild_callback_sub();
                }
            });
        }

        // Initial reconciliation: catch entities already cached before subscription was registered.
        {
            gtk::glib::idle_add_local_once(move || {
                let entities: Vec<(Urn, E)> = store_ref.get_entities_typed(entity_type);
                let now_available = !entities.is_empty();
                if now_available && !available_ref.get() {
                    if let Some((_urn, entity)) = entities.first() {
                        let update = on_update(entity);
                        toggle_ref.set_active(update.active);
                        toggle_ref.set_details(update.details);
                        if let Some(icon) = update.icon {
                            toggle_ref.set_icon(icon);
                        }
                    }
                    available_ref.set(true);
                    rebuild_callback();
                }
            });
        }

        Self {
            toggle,
            available,
            widget_id: config.widget_id,
            weight: config.weight,
        }
    }

    pub fn as_feature_toggles(&self) -> Vec<Rc<WidgetFeatureToggle>> {
        if !self.available.get() {
            return Vec::new();
        }
        vec![Rc::new(WidgetFeatureToggle {
            id: self.widget_id.to_string(),
            weight: self.weight,
            toggle: (*self.toggle).clone(),
            menu: None,
        })]
    }
}
