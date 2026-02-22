//! Tethering connection rows for hotspot client adapters.

use std::cell::RefCell;
use std::rc::Rc;

use waft_client::{EntityActionCallback, EntityStore};
use waft_protocol::Urn;
use waft_protocol::entity;
use waft_ui_gtk::vdom::Component;
use waft_ui_gtk::widgets::connection_row::{
    ConnectionRow, ConnectionRowOutput, ConnectionRowProps,
};

use super::{NetworkRow, ToggleEntry};

/// Update tethering connection rows in the tethering adapter toggle.
pub(super) fn update_tethering_menus(
    entries: &Rc<RefCell<Vec<ToggleEntry>>>,
    store: &Rc<EntityStore>,
    action_callback: &EntityActionCallback,
) {
    let connections: Vec<(Urn, entity::network::TetheringConnection)> =
        store.get_entities_typed(entity::network::TETHERING_CONNECTION_ENTITY_TYPE);

    let entries_mut = entries.borrow();

    // Find the tethering adapter toggle
    let tethering_urn_suffix = "/network-adapter/tethering";
    for entry in entries_mut.iter() {
        if !entry.urn_str.ends_with(tethering_urn_suffix) {
            continue;
        }

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
                row.remove_from(&entry.menu.root());
                false
            }
        });

        // Remove-and-recreate rows on state change so click closures have fresh state
        for (conn_urn, conn) in &adapter_connections {
            let conn_urn_str = conn_urn.as_str().to_string();

            if let Some(existing) = network_rows.iter().find(|r| r.urn_str() == conn_urn_str) {
                existing.remove_from(&entry.menu.root());
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
