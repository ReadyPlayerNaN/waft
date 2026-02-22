//! Tethering adapter toggles.
//!
//! Subscribes to `network-adapter` (filtered to `AdapterKind::Tethering`) and
//! `tethering-connection` entity types. Creates one FeatureToggleWidget per
//! tethering adapter.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use waft_client::{EntityActionCallback, EntityStore};
use waft_protocol::Urn;
use waft_protocol::entity;
use waft_ui_gtk::menu_state::menu_id_for_widget;
use waft_ui_gtk::vdom::Component;
use waft_ui_gtk::widgets::connection_row::{
    ConnectionRow, ConnectionRowOutput, ConnectionRowProps,
};
use waft_ui_gtk::widgets::feature_toggle::{FeatureToggleProps, FeatureToggleWidget};

use super::{NetworkRow, ToggleEntry, adapter_icon, adapter_title};
use crate::layout::types::WidgetFeatureToggle;
use crate::ui::feature_toggles::menu::FeatureToggleMenuWidget;

/// Dynamic set of toggles for tethering adapters.
pub struct TetheringToggles {
    entries: Rc<RefCell<Vec<ToggleEntry>>>,
    #[allow(dead_code)]
    store: Rc<EntityStore>,
    #[allow(dead_code)]
    action_callback: EntityActionCallback,
    #[allow(dead_code)]
    menu_store: Rc<waft_core::menu_state::MenuStore>,
}

impl TetheringToggles {
    pub fn new(
        store: &Rc<EntityStore>,
        action_callback: &EntityActionCallback,
        menu_store: &Rc<waft_core::menu_state::MenuStore>,
        rebuild_callback: Rc<dyn Fn()>,
    ) -> Self {
        let entries: Rc<RefCell<Vec<ToggleEntry>>> = Rc::new(RefCell::new(Vec::new()));

        // Subscribe to network adapter changes (tethering only)
        {
            let store_ref = store.clone();
            let entries_ref = entries.clone();
            let cb = action_callback.clone();
            let rebuild = rebuild_callback.clone();
            let menu_store_ref = menu_store.clone();

            store.subscribe_type(entity::network::ADAPTER_ENTITY_TYPE, move || {
                let adapters: Vec<(Urn, entity::network::NetworkAdapter)> =
                    store_ref.get_entities_typed(entity::network::ADAPTER_ENTITY_TYPE);

                // Filter to tethering adapters only
                let adapters: Vec<_> = adapters
                    .into_iter()
                    .filter(|(_, a)| matches!(a.kind, entity::network::AdapterKind::Tethering))
                    .collect();

                let mut entries_mut = entries_ref.borrow_mut();
                let mut changed = false;

                // Current adapter URN strings
                let current_urns: Vec<String> = adapters
                    .iter()
                    .map(|(urn, _)| urn.as_str().to_string())
                    .collect();

                // Remove adapter toggles that no longer exist
                let before_len = entries_mut.len();
                entries_mut.retain(|entry| current_urns.contains(&entry.urn_str));
                if entries_mut.len() != before_len {
                    changed = true;
                }

                // Update existing or create new adapter toggles
                for (urn, adapter) in &adapters {
                    let urn_str = urn.as_str().to_string();
                    let icon = adapter_icon(adapter);
                    let title = adapter_title(adapter);

                    if let Some(entry) = entries_mut.iter().find(|e| e.urn_str == urn_str) {
                        // Update existing toggle
                        entry.toggle.set_active(adapter.connected);
                        entry.toggle.set_busy(false);
                        entry.toggle.set_icon(&icon);
                        entry.connected.set(adapter.connected);
                    } else {
                        // Create new toggle for this adapter
                        let widget_id = format!("tethering-toggle-{}", urn_str);
                        let menu_id = menu_id_for_widget(&widget_id);

                        let menu = FeatureToggleMenuWidget::new();
                        let connected = Rc::new(Cell::new(adapter.connected));

                        let toggle = Rc::new(FeatureToggleWidget::new(
                            FeatureToggleProps {
                                active: adapter.connected,
                                busy: false,
                                details: None,
                                expandable: false,
                                icon,
                                title,
                                menu_id: Some(menu_id.clone()),
                                expanded: false,
                            },
                            Some(menu_store_ref.clone()),
                        ));

                        let action_cb = cb.clone();
                        let action_urn = urn.clone();
                        let connected_for_click = connected.clone();
                        toggle.connect_output(move |_output| {
                            let action = if connected_for_click.get() {
                                "deactivate"
                            } else {
                                "activate"
                            };
                            action_cb(
                                action_urn.clone(),
                                action.to_string(),
                                serde_json::Value::Null,
                            );
                        });

                        let entry = ToggleEntry {
                            urn_str,
                            toggle,
                            menu,
                            network_rows: RefCell::new(Vec::new()),
                            info_rows: RefCell::new(Vec::new()),
                            weight: 150,
                            connected,
                            settings_button: None,
                            settings_button_label: None,
                        };

                        entries_mut.push(entry);
                        changed = true;
                    }
                }

                if changed {
                    drop(entries_mut);
                    rebuild();
                }
            });
        }

        // Subscribe to tethering connection changes
        {
            let entries_ref = entries.clone();
            let store_ref = store.clone();
            let cb = action_callback.clone();
            store.subscribe_type(
                entity::network::TETHERING_CONNECTION_ENTITY_TYPE,
                move || {
                    update_tethering_menus(&entries_ref, &store_ref, &cb);
                },
            );
        }

        Self {
            entries,
            store: store.clone(),
            action_callback: action_callback.clone(),
            menu_store: menu_store.clone(),
        }
    }

    /// Return all current toggles as feature toggle widgets for the grid.
    pub fn as_feature_toggles(&self) -> Vec<Rc<WidgetFeatureToggle>> {
        self.entries
            .borrow()
            .iter()
            .map(|entry| {
                Rc::new(WidgetFeatureToggle {
                    id: format!("tethering-toggle-{}", entry.urn_str),
                    weight: entry.weight,
                    toggle: (*entry.toggle).clone(),
                    menu: Some(entry.menu.widget().clone()),
                })
            })
            .collect()
    }
}

/// Update tethering connection rows in the tethering adapter toggle.
fn update_tethering_menus(
    entries: &Rc<RefCell<Vec<ToggleEntry>>>,
    store: &Rc<EntityStore>,
    action_callback: &EntityActionCallback,
) {
    let connections: Vec<(Urn, entity::network::TetheringConnection)> =
        store.get_entities_typed(entity::network::TETHERING_CONNECTION_ENTITY_TYPE);

    let entries_mut = entries.borrow();

    for entry in entries_mut.iter() {
        // Find connections for this adapter by checking URN prefix
        let adapter_urn_prefix = format!("{}/", entry.urn_str);
        let adapter_connections: Vec<_> = connections
            .iter()
            .filter(|(urn, _)| urn.as_str().starts_with(&adapter_urn_prefix))
            .collect();

        entry.toggle.set_expandable(!adapter_connections.is_empty());

        // Update details text
        if let Some((_, active_conn)) = adapter_connections.iter().find(|(_, c)| c.active) {
            entry.toggle.set_details(Some(active_conn.name.clone()));
        } else {
            entry.toggle.set_details(None);
        }

        let mut network_rows = entry.network_rows.borrow_mut();

        // Remove rows for connections that no longer exist
        let current_urns: Vec<String> = adapter_connections
            .iter()
            .map(|(urn, _)| urn.as_str().to_string())
            .collect();
        network_rows.retain(|row| {
            if current_urns.iter().any(|u| u == row.urn_str()) {
                true
            } else {
                row.remove_from(entry.menu.root());
                false
            }
        });

        // Remove-and-recreate rows on state change so click closures have fresh state
        for (conn_urn, conn) in &adapter_connections {
            let conn_urn_str = conn_urn.as_str().to_string();

            if let Some(existing) = network_rows.iter().find(|r| r.urn_str() == conn_urn_str) {
                existing.remove_from(entry.menu.root());
                network_rows.retain(|r| r.urn_str() != conn_urn_str);
            }

            let conn_row = Rc::new(ConnectionRow::build(&ConnectionRowProps {
                name: conn.name.clone(),
                active: conn.active,
                transitioning: false,
                icon: None,
            }));

            let action_cb = action_callback.clone();
            let urn_for_click = (*conn_urn).clone();
            let is_active = conn.active;
            conn_row.connect_output(move |ConnectionRowOutput::Toggle| {
                let action = if is_active { "disconnect" } else { "connect" };
                action_cb(
                    urn_for_click.clone(),
                    action.to_string(),
                    serde_json::Value::Null,
                );
            });

            entry.menu.append(&conn_row.widget());

            network_rows.push(NetworkRow::Connection {
                urn_str: conn_urn_str,
                row: conn_row,
            });
        }
    }
}
