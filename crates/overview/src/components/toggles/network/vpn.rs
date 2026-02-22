//! VPN toggle.
//!
//! Subscribes to the `vpn` entity type. Presents a single consolidated VPN toggle
//! with expandable menu showing individual VPN connections.

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

use super::{NetworkRow, ToggleEntry};
use crate::layout::types::WidgetFeatureToggle;
use crate::ui::feature_toggles::menu::FeatureToggleMenuWidget;

/// Dynamic toggle for VPN connections.
pub struct VpnToggles {
    entries: Rc<RefCell<Vec<ToggleEntry>>>,
    #[allow(dead_code)]
    store: Rc<EntityStore>,
    #[allow(dead_code)]
    action_callback: EntityActionCallback,
    #[allow(dead_code)]
    menu_store: Rc<waft_core::menu_state::MenuStore>,
}

impl VpnToggles {
    pub fn new(
        store: &Rc<EntityStore>,
        action_callback: &EntityActionCallback,
        menu_store: &Rc<waft_core::menu_state::MenuStore>,
        rebuild_callback: Rc<dyn Fn()>,
    ) -> Self {
        let entries: Rc<RefCell<Vec<ToggleEntry>>> = Rc::new(RefCell::new(Vec::new()));

        // Subscribe to VPN changes - single consolidated toggle
        {
            let store_ref = store.clone();
            let entries_ref = entries.clone();
            let cb = action_callback.clone();
            let rebuild = rebuild_callback.clone();
            let menu_store_ref = menu_store.clone();

            // Track VPN URNs + states for the click handler
            let vpn_states: Rc<RefCell<Vec<(Urn, entity::network::VpnState)>>> =
                Rc::new(RefCell::new(Vec::new()));

            store.subscribe_type(entity::network::VPN_ENTITY_TYPE, move || {
                let vpns: Vec<(Urn, entity::network::Vpn)> =
                    store_ref.get_entities_typed(entity::network::VPN_ENTITY_TYPE);

                // Update tracked VPN states
                {
                    let mut states = vpn_states.borrow_mut();
                    states.clear();
                    for (urn, vpn) in &vpns {
                        states.push((urn.clone(), vpn.state));
                    }
                }

                let mut entries_mut = entries_ref.borrow_mut();

                if vpns.is_empty() {
                    // Remove consolidated VPN toggle if no VPNs exist
                    let before_len = entries_mut.len();
                    entries_mut.retain(|entry| entry.urn_str != "vpn-consolidated");
                    if entries_mut.len() != before_len {
                        drop(entries_mut);
                        rebuild();
                    }
                    return;
                }

                // Compute consolidated state
                let any_active = vpns.iter().any(|(_urn, vpn)| {
                    matches!(
                        vpn.state,
                        entity::network::VpnState::Connected
                            | entity::network::VpnState::Connecting
                    )
                });
                let any_busy = vpns.iter().any(|(_urn, vpn)| {
                    matches!(
                        vpn.state,
                        entity::network::VpnState::Connecting
                            | entity::network::VpnState::Disconnecting
                    )
                });
                let details = vpns
                    .iter()
                    .find(|(_, vpn)| vpn.state == entity::network::VpnState::Connected)
                    .map(|(_, vpn)| vpn.name.clone());

                if let Some(entry) =
                    entries_mut.iter().find(|e| e.urn_str == "vpn-consolidated")
                {
                    // Update existing consolidated toggle
                    entry.toggle.set_active(any_active);
                    entry.toggle.set_busy(any_busy);
                    entry.toggle.set_details(details.clone());
                    entry.toggle.set_expandable(!vpns.is_empty());

                    // Update VPN menu rows
                    update_vpn_menu_rows(entry, &vpns, &cb);
                } else {
                    // Create consolidated VPN toggle
                    let widget_id = "vpn-toggle-consolidated";
                    let menu_id = menu_id_for_widget(widget_id);

                    let menu = FeatureToggleMenuWidget::new();
                    let toggle = Rc::new(FeatureToggleWidget::new(
                        FeatureToggleProps {
                            active: any_active,
                            busy: any_busy,
                            details,
                            expandable: !vpns.is_empty(),
                            icon: "network-vpn-symbolic".to_string(),
                            title: "VPN".to_string(),
                            menu_id: Some(menu_id.clone()),
                        },
                        Some(menu_store_ref.clone()),
                    ));

                    // Toggle click: disconnect ALL connected VPNs
                    let action_cb = cb.clone();
                    let vpn_states_for_click = vpn_states.clone();
                    toggle.connect_output(move |_output| {
                        let states = vpn_states_for_click.borrow();
                        for (urn, state) in states.iter() {
                            if matches!(
                                state,
                                entity::network::VpnState::Connected
                                    | entity::network::VpnState::Connecting
                            ) {
                                action_cb(
                                    urn.clone(),
                                    "disconnect".to_string(),
                                    serde_json::Value::Null,
                                );
                            }
                        }
                    });

                    let entry = ToggleEntry {
                        urn_str: "vpn-consolidated".to_string(),
                        toggle,
                        menu,
                        network_rows: RefCell::new(Vec::new()),
                        info_rows: RefCell::new(Vec::new()),
                        weight: 160,
                        connected: Rc::new(Cell::new(any_active)),
                        settings_button: None,
                        settings_button_label: None,
                    };

                    // Populate VPN menu rows
                    update_vpn_menu_rows(&entry, &vpns, &cb);

                    entries_mut.push(entry);
                    drop(entries_mut);
                    rebuild();
                }
            });
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
                    id: format!("vpn-toggle-{}", entry.urn_str),
                    weight: entry.weight,
                    toggle: (*entry.toggle).clone(),
                    menu: Some(entry.menu.widget().clone()),
                })
            })
            .collect()
    }
}

/// Update VPN menu rows inside the consolidated VPN toggle.
///
/// Uses ConnectionRow widgets with incremental updates instead of
/// full drain+recreate.
fn update_vpn_menu_rows(
    entry: &ToggleEntry,
    vpns: &[(Urn, entity::network::Vpn)],
    action_callback: &EntityActionCallback,
) {
    let mut network_rows = entry.network_rows.borrow_mut();

    // Remove rows for VPNs that no longer exist
    let current_vpn_urns: Vec<String> = vpns
        .iter()
        .map(|(urn, _)| urn.as_str().to_string())
        .collect();
    network_rows.retain(|row| {
        if current_vpn_urns.iter().any(|u| u == row.urn_str()) {
            true
        } else {
            row.remove_from(entry.menu.root());
            false
        }
    });

    // Update existing or create new rows
    for (vpn_urn, vpn) in vpns {
        let vpn_urn_str = vpn_urn.as_str().to_string();
        let active = vpn.state == entity::network::VpnState::Connected;
        let transitioning = matches!(
            vpn.state,
            entity::network::VpnState::Connecting | entity::network::VpnState::Disconnecting
        );

        if let Some(existing) = network_rows.iter().find(|r| r.urn_str() == vpn_urn_str) {
            // Update existing ConnectionRow
            if let NetworkRow::Connection { row, .. } = existing {
                row.update(&ConnectionRowProps {
                    name: vpn.name.clone(),
                    active,
                    transitioning,
                    icon: Some(vpn_icon_name(&vpn.vpn_type)),
                });
            }
        } else {
            // Create new ConnectionRow
            let conn_row = Rc::new(ConnectionRow::build(&ConnectionRowProps {
                name: vpn.name.clone(),
                active,
                transitioning,
                icon: Some(vpn_icon_name(&vpn.vpn_type)),
            }));

            let action_cb = action_callback.clone();
            let urn_for_click = vpn_urn.clone();
            let vpn_state = vpn.state;
            conn_row.connect_output(move |ConnectionRowOutput::Toggle| {
                let action = match vpn_state {
                    entity::network::VpnState::Connected => "disconnect",
                    entity::network::VpnState::Disconnected => "connect",
                    // Don't send actions during transitions
                    _ => return,
                };
                action_cb(
                    urn_for_click.clone(),
                    action.to_string(),
                    serde_json::Value::Null,
                );
            });

            entry.menu.append(&conn_row.widget());

            network_rows.push(NetworkRow::Connection {
                urn_str: vpn_urn_str,
                row: conn_row,
            });
        }
    }
}

/// Determine the icon name for a VPN connection based on its type.
fn vpn_icon_name(vpn_type: &entity::network::VpnType) -> String {
    match vpn_type {
        entity::network::VpnType::Wireguard => "network-vpn-symbolic".to_string(),
        entity::network::VpnType::Vpn => "network-vpn-symbolic".to_string(),
    }
}
