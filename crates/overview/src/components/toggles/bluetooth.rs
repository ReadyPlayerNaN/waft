//! Bluetooth adapter toggle components.
//!
//! Subscribes to the `bluetooth-adapter` entity type and dynamically creates
//! one FeatureToggleWidget per adapter. Adapters that appear or disappear are
//! tracked and the toggle set is kept in sync.

use std::cell::RefCell;
use std::rc::Rc;

use waft_protocol::entity;
use waft_protocol::Urn;
use waft_ui_gtk::widgets::feature_toggle::{FeatureToggleProps, FeatureToggleWidget};

use crate::entity_store::{EntityActionCallback, EntityStore};
use crate::plugin::WidgetFeatureToggle;

/// A tracked toggle entry for a single Bluetooth adapter.
struct ToggleEntry {
    urn_str: String,
    toggle: Rc<FeatureToggleWidget>,
}

/// Dynamic set of toggles for Bluetooth adapters (0..N).
///
/// Maintains one FeatureToggleWidget per adapter entity. When the entity set
/// changes, existing toggles are updated in place and new ones are created
/// or stale ones removed.
pub struct BluetoothToggles {
    entries: Rc<RefCell<Vec<ToggleEntry>>>,
}

impl BluetoothToggles {
    /// Create a new BluetoothToggles that subscribes to the entity store.
    ///
    /// `rebuild_callback` is invoked whenever the set of toggles changes
    /// (adapter added or removed) so the parent grid can rebuild.
    pub fn new(
        store: &Rc<EntityStore>,
        action_callback: &EntityActionCallback,
        rebuild_callback: Rc<dyn Fn()>,
    ) -> Self {
        let entries: Rc<RefCell<Vec<ToggleEntry>>> = Rc::new(RefCell::new(Vec::new()));

        let store_ref = store.clone();
        let entries_ref = entries.clone();
        let cb = action_callback.clone();
        let rebuild = rebuild_callback.clone();

        store.subscribe_type(entity::bluetooth::BluetoothAdapter::ENTITY_TYPE, move || {
            let adapters: Vec<(Urn, entity::bluetooth::BluetoothAdapter)> =
                store_ref.get_entities_typed(entity::bluetooth::BluetoothAdapter::ENTITY_TYPE);

            let mut entries_mut = entries_ref.borrow_mut();
            let mut changed = false;

            // Build a set of current URN strings for quick lookup
            let current_urns: Vec<String> = adapters
                .iter()
                .map(|(urn, _)| urn.as_str().to_string())
                .collect();

            // Remove toggles for adapters that no longer exist
            let before_len = entries_mut.len();
            entries_mut.retain(|entry| current_urns.contains(&entry.urn_str));
            if entries_mut.len() != before_len {
                changed = true;
            }

            // Update existing or create new toggles
            for (urn, adapter) in &adapters {
                let urn_str = urn.as_str().to_string();
                let icon = if adapter.powered {
                    "bluetooth-active-symbolic"
                } else {
                    "bluetooth-disabled-symbolic"
                };

                if let Some(entry) = entries_mut.iter().find(|e| e.urn_str == urn_str) {
                    // Update existing toggle
                    entry.toggle.set_active(adapter.powered);
                    entry.toggle.set_icon(icon);
                    entry.toggle.set_details(Some(adapter.name.clone()));
                } else {
                    // Create new toggle for this adapter
                    let toggle = Rc::new(FeatureToggleWidget::new(
                        FeatureToggleProps {
                            active: adapter.powered,
                            busy: false,
                            details: Some(adapter.name.clone()),
                            expandable: false,
                            icon: icon.to_string(),
                            title: "Bluetooth".to_string(),
                            menu_id: None,
                        },
                        None,
                    ));

                    let action_cb = cb.clone();
                    let action_urn = urn.clone();
                    toggle.connect_output(move |_output| {
                        action_cb(
                            action_urn.clone(),
                            "toggle-power".to_string(),
                            serde_json::Value::Null,
                        );
                    });

                    entries_mut.push(ToggleEntry {
                        urn_str,
                        toggle,
                    });
                    changed = true;
                }
            }

            // Notify the parent grid if the toggle set changed
            if changed {
                drop(entries_mut);
                rebuild();
            }
        });

        Self { entries }
    }

    /// Return all current toggles as feature toggle widgets for the grid.
    pub fn as_feature_toggles(&self) -> Vec<Rc<WidgetFeatureToggle>> {
        self.entries
            .borrow()
            .iter()
            .enumerate()
            .map(|(i, entry)| {
                Rc::new(WidgetFeatureToggle {
                    id: format!("bluetooth-toggle-{}", entry.urn_str),
                    weight: 500 + i as i32,
                    el: entry.toggle.widget(),
                    menu: None,
                    on_expand_toggled: None,
                    menu_id: None,
                })
            })
            .collect()
    }
}
