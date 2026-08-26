//! VPN toggle.
//!
//! Subscribes to the `vpn` entity type. Presents a single consolidated VPN toggle
//! with expandable menu showing individual VPN connections.

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;

use waft_client::{EntityActionCallback, EntityStore};
use waft_protocol::Urn;
use waft_protocol::entity;
use waft_ui_gtk::menu_state::{menu_id_for_widget, toggle_menu};
use waft_ui_gtk::vdom::Component;
use waft_ui_gtk::widgets::connection_row::{
    ConnectionRow, ConnectionRowOutput, ConnectionRowProps,
};
use waft_ui_gtk::widgets::feature_toggle::{
    FeatureToggleOutput, FeatureToggleProps, FeatureToggleWidget,
};

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

#[derive(Debug, Clone, PartialEq, Eq)]
struct VpnSummary {
    active: bool,
    busy: bool,
    details: Option<String>,
}

fn vpn_toggle_activate_target(
    vpn_states: &HashMap<String, entity::network::VpnState>,
    last_active_urn: Option<&str>,
) -> Option<Urn> {
    if let Some(last) = last_active_urn
        && matches!(
            vpn_states.get(last),
            Some(entity::network::VpnState::Disconnected)
        )
    {
        return Urn::parse(last).ok();
    }

    vpn_states.iter().find_map(|(urn, state)| {
        (*state == entity::network::VpnState::Disconnected)
            .then(|| Urn::parse(urn).ok())
            .flatten()
    })
}

fn vpn_toggle_deactivate_targets(
    vpn_states: &HashMap<String, entity::network::VpnState>,
) -> Vec<Urn> {
    vpn_states
        .iter()
        .filter_map(|(urn, state)| {
            matches!(
                state,
                entity::network::VpnState::Connected | entity::network::VpnState::Connecting
            )
            .then(|| Urn::parse(urn).ok())
            .flatten()
        })
        .collect()
}

fn vpn_action_for_state(state: entity::network::VpnState) -> Option<&'static str> {
    match state {
        entity::network::VpnState::Connected => Some("disconnect"),
        entity::network::VpnState::Disconnected => Some("connect"),
        entity::network::VpnState::Connecting | entity::network::VpnState::Disconnecting => None,
    }
}

fn summarize_vpns(vpns: &[(Urn, entity::network::Vpn)]) -> VpnSummary {
    let any_connected = vpns
        .iter()
        .any(|(_, vpn)| vpn.state == entity::network::VpnState::Connected);
    let any_connecting = vpns
        .iter()
        .any(|(_, vpn)| vpn.state == entity::network::VpnState::Connecting);
    let details = vpns
        .iter()
        .find(|(_, vpn)| vpn.state == entity::network::VpnState::Disconnecting)
        .map(|(_, vpn)| format!("{} — {}", vpn.name, crate::i18n::t("vpn-disconnecting")))
        .or_else(|| {
            vpns.iter()
                .find(|(_, vpn)| vpn.state == entity::network::VpnState::Connecting)
                .map(|(_, vpn)| format!("{} — {}", vpn.name, crate::i18n::t("vpn-connecting")))
        })
        .or_else(|| {
            vpns.iter()
                .find(|(_, vpn)| vpn.state == entity::network::VpnState::Connected)
                .map(|(_, vpn)| vpn.name.clone())
        })
        .or_else(|| Some(crate::i18n::t("vpn-disconnected")));

    VpnSummary {
        active: any_connected || any_connecting,
        busy: false,
        details,
    }
}

impl VpnToggles {
    pub fn new(
        store: &Rc<EntityStore>,
        action_callback: &EntityActionCallback,
        menu_store: &Rc<waft_core::menu_state::MenuStore>,
        rebuild_callback: &Rc<dyn Fn()>,
    ) -> Self {
        let entries: Rc<RefCell<Vec<ToggleEntry>>> = Rc::new(RefCell::new(Vec::new()));

        // Subscribe to VPN changes - single consolidated toggle
        {
            let store_ref = store.clone();
            let entries_ref = entries.clone();
            let cb = action_callback.clone();
            let rebuild = rebuild_callback.clone();
            let menu_store_ref = menu_store.clone();

            // Track VPN URNs + current states for click handlers.
            let vpn_states: Rc<RefCell<HashMap<String, entity::network::VpnState>>> =
                Rc::new(RefCell::new(HashMap::new()));
            let last_active_vpn: Rc<RefCell<Option<String>>> = Rc::new(RefCell::new(None));

            store.subscribe_type(entity::network::VPN_ENTITY_TYPE, move || {
                let vpns: Vec<(Urn, entity::network::Vpn)> =
                    store_ref.get_entities_typed(entity::network::VPN_ENTITY_TYPE);

                // Update tracked VPN states
                {
                    let mut states = vpn_states.borrow_mut();
                    states.clear();
                    for (urn, vpn) in &vpns {
                        states.insert(urn.as_str().to_string(), vpn.state);
                    }
                }
                if let Some((urn, _)) = vpns.iter().find(|(_, vpn)| {
                    matches!(
                        vpn.state,
                        entity::network::VpnState::Connected
                            | entity::network::VpnState::Connecting
                    )
                }) {
                    *last_active_vpn.borrow_mut() = Some(urn.as_str().to_string());
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

                let summary = summarize_vpns(&vpns);

                if let Some(entry) = entries_mut.iter().find(|e| e.urn_str == "vpn-consolidated") {
                    // Update existing consolidated toggle
                    entry.toggle.set_active(summary.active);
                    entry.toggle.set_busy(summary.busy);
                    entry.toggle.set_details(summary.details.clone());
                    entry.toggle.set_expandable(!vpns.is_empty());

                    // Update VPN menu rows
                    update_vpn_menu_rows(entry, &vpns, &cb, &vpn_states);
                } else {
                    // Create consolidated VPN toggle
                    let widget_id = "vpn-toggle-consolidated";
                    let menu_id = menu_id_for_widget(widget_id);

                    let menu = FeatureToggleMenuWidget::new();
                    let toggle = Rc::new(FeatureToggleWidget::new(
                        FeatureToggleProps {
                            active: summary.active,
                            busy: summary.busy,
                            details: summary.details.clone(),
                            expandable: !vpns.is_empty(),
                            icon: "network-vpn-symbolic".to_string(),
                            title: crate::i18n::t("vpn-title"),
                            menu_id: Some(menu_id.clone()),
                            expanded: false,
                        },
                        Some(menu_store_ref.clone()),
                    ));

                    // Toggle click: disconnect ALL connected VPNs
                    let action_cb = cb.clone();
                    let vpn_states_for_click = vpn_states.clone();
                    let last_active_vpn_for_click = last_active_vpn.clone();
                    let menu_id_for_expand = menu_id.clone();
                    let menu_store_for_expand = menu_store_ref.clone();
                    toggle.connect_output(move |output| match output {
                        FeatureToggleOutput::Activate => {
                            if let Some(urn) = vpn_toggle_activate_target(
                                &vpn_states_for_click.borrow(),
                                last_active_vpn_for_click.borrow().as_deref(),
                            ) {
                                action_cb(urn, "connect".to_string(), serde_json::Value::Null);
                            }
                        }
                        FeatureToggleOutput::Deactivate => {
                            for urn in vpn_toggle_deactivate_targets(&vpn_states_for_click.borrow())
                            {
                                action_cb(urn, "disconnect".to_string(), serde_json::Value::Null);
                            }
                        }
                        FeatureToggleOutput::ExpandToggle(_) => {
                            toggle_menu(&menu_store_for_expand, &menu_id_for_expand);
                        }
                    });

                    let entry = ToggleEntry {
                        urn_str: "vpn-consolidated".to_string(),
                        toggle,
                        menu,
                        network_rows: RefCell::new(Vec::new()),
                        info_rows: RefCell::new(Vec::new()),
                        weight: 160,
                        connected: Rc::new(Cell::new(summary.active)),
                        settings_button: None,
                        settings_button_label: None,
                    };

                    // Populate VPN menu rows
                    update_vpn_menu_rows(&entry, &vpns, &cb, &vpn_states);

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
    vpn_states: &Rc<RefCell<HashMap<String, entity::network::VpnState>>>,
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
            let vpn_states_for_click = vpn_states.clone();
            conn_row.connect_output(move |ConnectionRowOutput::Toggle| {
                let current_state = vpn_states_for_click
                    .borrow()
                    .get(urn_for_click.as_str())
                    .copied()
                    .unwrap_or(entity::network::VpnState::Disconnected);
                let Some(action) = vpn_action_for_state(current_state) else {
                    return;
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

#[cfg(test)]
mod tests {
    use super::*;

    fn make_vpn(name: &str, state: entity::network::VpnState) -> (Urn, entity::network::Vpn) {
        (
            Urn::new("networkmanager", entity::network::VPN_ENTITY_TYPE, name),
            entity::network::Vpn {
                name: name.to_string(),
                state,
                vpn_type: entity::network::VpnType::Wireguard,
            },
        )
    }

    #[test]
    fn aggregate_summary_prefers_transition_details_without_global_busy() {
        let vpns = vec![
            make_vpn("kiwi", entity::network::VpnState::Disconnecting),
            make_vpn("office", entity::network::VpnState::Connected),
        ];

        let summary = summarize_vpns(&vpns);

        assert!(summary.active);
        assert!(
            !summary.busy,
            "one transitioning VPN must not globally busy-lock the tile"
        );
        assert_eq!(
            summary.details,
            Some(format!("kiwi — {}", crate::i18n::t("vpn-disconnecting")))
        );
    }

    #[test]
    fn aggregate_summary_marks_connecting_as_active() {
        let vpns = vec![make_vpn("kiwi", entity::network::VpnState::Connecting)];
        let summary = summarize_vpns(&vpns);
        assert!(summary.active);
        assert_eq!(
            summary.details,
            Some(format!("kiwi — {}", crate::i18n::t("vpn-connecting")))
        );
    }

    #[test]
    fn action_selection_is_per_vpn_state() {
        assert_eq!(
            vpn_action_for_state(entity::network::VpnState::Disconnected),
            Some("connect")
        );
        assert_eq!(
            vpn_action_for_state(entity::network::VpnState::Connected),
            Some("disconnect")
        );
        assert_eq!(
            vpn_action_for_state(entity::network::VpnState::Connecting),
            None
        );
        assert_eq!(
            vpn_action_for_state(entity::network::VpnState::Disconnecting),
            None
        );
    }

    #[test]
    fn consolidated_toggle_reconnects_last_active_vpn_first() {
        let states = HashMap::from([
            (
                Urn::new("networkmanager", entity::network::VPN_ENTITY_TYPE, "kiwi")
                    .as_str()
                    .to_string(),
                entity::network::VpnState::Disconnected,
            ),
            (
                Urn::new("networkmanager", entity::network::VPN_ENTITY_TYPE, "home")
                    .as_str()
                    .to_string(),
                entity::network::VpnState::Disconnected,
            ),
        ]);
        let target = vpn_toggle_activate_target(
            &states,
            Some(Urn::new("networkmanager", entity::network::VPN_ENTITY_TYPE, "home").as_str()),
        )
        .expect("target");
        assert_eq!(target.id(), "home");
    }

    #[test]
    fn consolidated_toggle_disconnects_connected_and_connecting_vpns() {
        let states = HashMap::from([
            (
                Urn::new("networkmanager", entity::network::VPN_ENTITY_TYPE, "kiwi")
                    .as_str()
                    .to_string(),
                entity::network::VpnState::Connected,
            ),
            (
                Urn::new("networkmanager", entity::network::VPN_ENTITY_TYPE, "home")
                    .as_str()
                    .to_string(),
                entity::network::VpnState::Connecting,
            ),
            (
                Urn::new("networkmanager", entity::network::VPN_ENTITY_TYPE, "lab")
                    .as_str()
                    .to_string(),
                entity::network::VpnState::Disconnected,
            ),
        ]);
        let targets = vpn_toggle_deactivate_targets(&states);
        assert_eq!(targets.len(), 2);
    }
}
