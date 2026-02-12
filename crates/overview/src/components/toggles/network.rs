//! Network adapter and VPN toggle components.
//!
//! Subscribes to both `network-adapter` and `vpn` entity types and dynamically
//! creates one FeatureToggleWidget per adapter/VPN. Entities that appear or
//! disappear are tracked and the toggle set is kept in sync.

use std::cell::RefCell;
use std::rc::Rc;

use waft_protocol::entity;
use waft_protocol::Urn;
use waft_ui_gtk::widgets::feature_toggle::{FeatureToggleProps, FeatureToggleWidget};

use crate::entity_store::{EntityActionCallback, EntityStore};
use crate::plugin::WidgetFeatureToggle;

/// A tracked toggle entry for a network adapter or VPN.
struct ToggleEntry {
    urn_str: String,
    toggle: Rc<FeatureToggleWidget>,
    weight: i32,
}

/// Dynamic set of toggles for network adapters and VPN connections.
///
/// Maintains one FeatureToggleWidget per network-adapter entity and one per
/// VPN entity. Subscribes to both entity types and keeps the toggle set
/// in sync as entities appear, change, or are removed.
pub struct NetworkManagerToggles {
    entries: Rc<RefCell<Vec<ToggleEntry>>>,
}

impl NetworkManagerToggles {
    /// Create a new NetworkManagerToggles that subscribes to the entity store.
    ///
    /// `rebuild_callback` is invoked whenever the set of toggles changes
    /// (adapter/VPN added or removed) so the parent grid can rebuild.
    pub fn new(
        store: &Rc<EntityStore>,
        action_callback: &EntityActionCallback,
        rebuild_callback: Rc<dyn Fn()>,
    ) -> Self {
        let entries: Rc<RefCell<Vec<ToggleEntry>>> = Rc::new(RefCell::new(Vec::new()));

        // Subscribe to network adapter changes
        {
            let store_ref = store.clone();
            let entries_ref = entries.clone();
            let cb = action_callback.clone();
            let rebuild = rebuild_callback.clone();

            store.subscribe_type(entity::network::ADAPTER_ENTITY_TYPE, move || {
                let adapters: Vec<(Urn, entity::network::NetworkAdapter)> =
                    store_ref.get_entities_typed(entity::network::ADAPTER_ENTITY_TYPE);

                let mut entries_mut = entries_ref.borrow_mut();
                let mut changed = false;

                // Current adapter URN strings
                let current_urns: Vec<String> = adapters
                    .iter()
                    .map(|(urn, _)| urn.as_str().to_string())
                    .collect();

                // Remove adapter toggles that no longer exist
                let before_len = entries_mut.len();
                entries_mut.retain(|entry| {
                    // Keep VPN entries (not our responsibility) and current adapter entries
                    !entry.urn_str.contains("/network-adapter/") || current_urns.contains(&entry.urn_str)
                });
                if entries_mut.len() != before_len {
                    changed = true;
                }

                // Update existing or create new adapter toggles
                for (urn, adapter) in &adapters {
                    let urn_str = urn.as_str().to_string();
                    let (icon, details) = adapter_icon_and_details(adapter);

                    if let Some(entry) = entries_mut.iter().find(|e| e.urn_str == urn_str) {
                        // Update existing toggle
                        entry.toggle.set_active(adapter.active);
                        entry.toggle.set_busy(false);
                        entry.toggle.set_icon(&icon);
                        entry.toggle.set_details(details);
                    } else {
                        // Create new toggle for this adapter
                        let toggle = Rc::new(FeatureToggleWidget::new(
                            FeatureToggleProps {
                                active: adapter.active,
                                busy: false,
                                details,
                                expandable: false,
                                icon,
                                title: "Network".to_string(),
                                menu_id: None,
                            },
                            None,
                        ));

                        let action_cb = cb.clone();
                        let action_urn = urn.clone();
                        toggle.connect_output(move |_output| {
                            action_cb(
                                action_urn.clone(),
                                "toggle".to_string(),
                                serde_json::Value::Null,
                            );
                        });

                        entries_mut.push(ToggleEntry {
                            urn_str,
                            toggle,
                            weight: 150,
                        });
                        changed = true;
                    }
                }

                if changed {
                    drop(entries_mut);
                    rebuild();
                }
            });
        }

        // Subscribe to VPN changes
        {
            let store_ref = store.clone();
            let entries_ref = entries.clone();
            let cb = action_callback.clone();
            let rebuild = rebuild_callback.clone();

            store.subscribe_type(entity::network::VPN_ENTITY_TYPE, move || {
                let vpns: Vec<(Urn, entity::network::Vpn)> =
                    store_ref.get_entities_typed(entity::network::VPN_ENTITY_TYPE);

                let mut entries_mut = entries_ref.borrow_mut();
                let mut changed = false;

                // Current VPN URN strings
                let current_urns: Vec<String> = vpns
                    .iter()
                    .map(|(urn, _)| urn.as_str().to_string())
                    .collect();

                // Remove VPN toggles that no longer exist
                let before_len = entries_mut.len();
                entries_mut.retain(|entry| {
                    // Keep adapter entries (not our responsibility) and current VPN entries
                    !entry.urn_str.contains("/vpn/") || current_urns.contains(&entry.urn_str)
                });
                if entries_mut.len() != before_len {
                    changed = true;
                }

                // Update existing or create new VPN toggles
                for (urn, vpn) in &vpns {
                    let urn_str = urn.as_str().to_string();
                    let active = matches!(
                        vpn.state,
                        entity::network::VpnState::Connected | entity::network::VpnState::Connecting
                    );
                    let busy = matches!(
                        vpn.state,
                        entity::network::VpnState::Connecting | entity::network::VpnState::Disconnecting
                    );

                    if let Some(entry) = entries_mut.iter().find(|e| e.urn_str == urn_str) {
                        // Update existing toggle
                        entry.toggle.set_active(active);
                        entry.toggle.set_busy(busy);
                        entry.toggle.set_details(Some(vpn.name.clone()));
                    } else {
                        // Create new toggle for this VPN
                        let toggle = Rc::new(FeatureToggleWidget::new(
                            FeatureToggleProps {
                                active,
                                busy,
                                details: Some(vpn.name.clone()),
                                expandable: false,
                                icon: "network-vpn-symbolic".to_string(),
                                title: "VPN".to_string(),
                                menu_id: None,
                            },
                            None,
                        ));

                        let action_cb = cb.clone();
                        let action_urn = urn.clone();
                        toggle.connect_output(move |_output| {
                            action_cb(
                                action_urn.clone(),
                                "toggle".to_string(),
                                serde_json::Value::Null,
                            );
                        });

                        entries_mut.push(ToggleEntry {
                            urn_str,
                            toggle,
                            weight: 160,
                        });
                        changed = true;
                    }
                }

                if changed {
                    drop(entries_mut);
                    rebuild();
                }
            });
        }

        Self { entries }
    }

    /// Return all current toggles as feature toggle widgets for the grid.
    pub fn as_feature_toggles(&self) -> Vec<Rc<WidgetFeatureToggle>> {
        self.entries
            .borrow()
            .iter()
            .map(|entry| {
                Rc::new(WidgetFeatureToggle {
                    id: format!("network-toggle-{}", entry.urn_str),
                    weight: entry.weight,
                    el: entry.toggle.widget(),
                    menu: None,
                    on_expand_toggled: None,
                    menu_id: None,
                })
            })
            .collect()
    }
}

/// Determine the icon and details text for a network adapter based on its kind.
fn adapter_icon_and_details(adapter: &entity::network::NetworkAdapter) -> (String, Option<String>) {
    match &adapter.kind {
        entity::network::AdapterKind::Wired { current_profile, .. } => {
            let icon = if adapter.active {
                "network-wired-symbolic"
            } else {
                "network-wired-disconnected-symbolic"
            };
            (icon.to_string(), current_profile.clone())
        }
        entity::network::AdapterKind::Wireless { connected, .. } => {
            let icon = match connected {
                Some(net) if net.strength > 75 => "network-wireless-signal-excellent-symbolic",
                Some(net) if net.strength > 50 => "network-wireless-signal-good-symbolic",
                Some(net) if net.strength > 25 => "network-wireless-signal-ok-symbolic",
                Some(_) => "network-wireless-signal-weak-symbolic",
                None => "network-wireless-offline-symbolic",
            };
            let name = connected.as_ref().map(|n| n.ssid.clone());
            (icon.to_string(), name)
        }
    }
}
