//! Network adapter and VPN toggle components.
//!
//! Subscribes to `network-adapter`, `wifi-network`, `ethernet-connection`, and `vpn`
//! entity types. Dynamically creates FeatureToggleWidget per adapter/VPN with expandable
//! menus showing child networks/connections.

mod network_menu_logic;
mod tethering;
mod vpn;
mod wifi;
mod wired;

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use gtk::prelude::*;
use waft_protocol::Urn;
use waft_protocol::entity;
use waft_ui_gtk::menu_state::menu_id_for_widget;
use waft_ui_gtk::widgets::connection_row::ConnectionRow;
use waft_ui_gtk::widgets::feature_toggle::{FeatureToggleProps, FeatureToggleWidget};

use crate::i18n;
use crate::layout::types::WidgetFeatureToggle;
use crate::ui::feature_toggles::menu::FeatureToggleMenuWidget;
use crate::ui::feature_toggles::menu_info_row::FeatureToggleMenuInfoRow;
use crate::ui::feature_toggles::menu_settings::{
    FeatureToggleMenuSettingsButton, FeatureToggleMenuSettingsButtonProps,
};
use waft_client::{EntityActionCallback, EntityStore};

/// A tracked toggle entry for a network adapter or VPN.
pub(super) struct ToggleEntry {
    urn_str: String,
    toggle: Rc<FeatureToggleWidget>,
    menu: FeatureToggleMenuWidget,
    network_rows: RefCell<Vec<NetworkRow>>,
    info_rows: RefCell<Vec<FeatureToggleMenuInfoRow>>,
    weight: i32,
    /// Tracks connected state for click handler closures that need fresh state.
    connected: Rc<Cell<bool>>,
    /// Settings button for wired adapter menus (None for WiFi/VPN/Tethering).
    settings_button: Option<FeatureToggleMenuSettingsButton>,
}

/// A single network row in the menu — either a plain box (WiFi/Ethernet)
/// or a ConnectionRow widget (VPN).
pub(super) enum NetworkRow {
    /// WiFi/Ethernet rows using plain gtk::Box layout.
    Plain { urn_str: String, root: gtk::Box },
    /// VPN rows using the extracted ConnectionRow widget.
    Connection {
        urn_str: String,
        row: Rc<ConnectionRow>,
    },
}

impl NetworkRow {
    fn urn_str(&self) -> &str {
        match self {
            NetworkRow::Plain { urn_str, .. } => urn_str,
            NetworkRow::Connection { urn_str, .. } => urn_str,
        }
    }

    fn remove_from(&self, parent: &gtk::Box) {
        match self {
            NetworkRow::Plain { root, .. } => parent.remove(root),
            NetworkRow::Connection { row, .. } => parent.remove(&row.root),
        }
    }
}

/// Dynamic set of toggles for network adapters and VPN connections.
///
/// Maintains one FeatureToggleWidget per network-adapter entity and one per
/// VPN entity. Subscribes to both entity types and keeps the toggle set
/// in sync as entities appear, change, or are removed.
pub struct NetworkManagerToggles {
    entries: Rc<RefCell<Vec<ToggleEntry>>>,
    #[allow(dead_code)]
    store: Rc<EntityStore>,
    #[allow(dead_code)]
    action_callback: EntityActionCallback,
    #[allow(dead_code)]
    menu_store: Rc<waft_core::menu_state::MenuStore>,
    /// Whether the settings app entity is available.
    #[allow(dead_code)]
    settings_available: Rc<Cell<bool>>,
}

impl NetworkManagerToggles {
    /// Create a new NetworkManagerToggles that subscribes to the entity store.
    ///
    /// `rebuild_callback` is invoked whenever the set of toggles changes
    /// (adapter/VPN added or removed) so the parent grid can rebuild.
    pub fn new(
        store: &Rc<EntityStore>,
        action_callback: &EntityActionCallback,
        menu_store: &Rc<waft_core::menu_state::MenuStore>,
        rebuild_callback: Rc<dyn Fn()>,
    ) -> Self {
        let entries: Rc<RefCell<Vec<ToggleEntry>>> = Rc::new(RefCell::new(Vec::new()));
        let settings_available: Rc<Cell<bool>> = Rc::new(Cell::new(false));
        let settings_urn: Rc<RefCell<Option<Urn>>> = Rc::new(RefCell::new(None));

        // Subscribe to network adapter changes
        {
            let store_ref = store.clone();
            let entries_ref = entries.clone();
            let cb = action_callback.clone();
            let rebuild = rebuild_callback.clone();
            let menu_store_ref = menu_store.clone();
            let settings_available_ref = settings_available.clone();
            let settings_urn_ref = settings_urn.clone();
            // Extra clones for update_wifi_menus call after entry changes
            let wifi_menu_store_ref = store.clone();
            let wifi_menu_entries_ref = entries.clone();
            let wifi_menu_cb = action_callback.clone();
            let wifi_menu_settings_ref = settings_available.clone();

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
                    !entry.urn_str.contains("/network-adapter/")
                        || current_urns.contains(&entry.urn_str)
                });
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

                        // Update IP info rows for wired adapters
                        if matches!(adapter.kind, entity::network::AdapterKind::Wired) {
                            wired::update_wired_info_rows(entry, adapter, &settings_available_ref);
                        }
                    } else {
                        // Create new toggle for this adapter
                        let widget_id = format!("network-toggle-{}", urn_str);
                        let menu_id = menu_id_for_widget(&widget_id);

                        // Create menu container for networks/connections
                        let menu = FeatureToggleMenuWidget::new();
                        let connected = Rc::new(Cell::new(adapter.connected));

                        let toggle = Rc::new(FeatureToggleWidget::new(
                            FeatureToggleProps {
                                active: adapter.connected,
                                busy: false,
                                details: None,
                                expandable: false, // Will be updated based on child count
                                icon,
                                title,
                                menu_id: Some(menu_id.clone()),
                            },
                            Some(menu_store_ref.clone()),
                        ));

                        let action_cb = cb.clone();
                        let action_urn = urn.clone();
                        let adapter_kind = adapter.kind.clone();
                        let connected_for_click = connected.clone();
                        toggle.connect_output(move |_output| {
                            let action = match adapter_kind {
                                entity::network::AdapterKind::Wireless => "activate",
                                entity::network::AdapterKind::Wired => "activate",
                                entity::network::AdapterKind::Tethering => {
                                    if connected_for_click.get() {
                                        "deactivate"
                                    } else {
                                        "activate"
                                    }
                                }
                            };
                            action_cb(
                                action_urn.clone(),
                                action.to_string(),
                                serde_json::Value::Null,
                            );
                        });

                        // Create settings button for wired and wireless adapters
                        let settings_button = match adapter.kind {
                            entity::network::AdapterKind::Wired => {
                                let btn = build_settings_button(
                                    &settings_urn_ref,
                                    &cb,
                                    "wired",
                                    "wired-settings-button",
                                );
                                menu.append(&btn.widget());
                                Some(btn)
                            }
                            entity::network::AdapterKind::Wireless => {
                                let btn = build_settings_button(
                                    &settings_urn_ref,
                                    &cb,
                                    "wifi",
                                    "wifi-settings-button",
                                );
                                menu.append(&btn.widget());
                                Some(btn)
                            }
                            _ => None,
                        };

                        let entry = ToggleEntry {
                            urn_str,
                            toggle,
                            menu,
                            network_rows: RefCell::new(Vec::new()),
                            info_rows: RefCell::new(Vec::new()),
                            weight: 150,
                            connected,
                            settings_button,
                        };

                        // Initialize IP info rows for wired adapters
                        if matches!(adapter.kind, entity::network::AdapterKind::Wired) {
                            wired::update_wired_info_rows(&entry, adapter, &settings_available_ref);
                        }

                        entries_mut.push(entry);
                        changed = true;
                    }
                }

                if changed {
                    drop(entries_mut);
                    // Re-evaluate WiFi menus immediately so newly-created toggle entries
                    // pick up the correct expandable state based on current has_settings
                    // and any wifi-network entities already in the store.
                    wifi::update_wifi_menus(
                        &wifi_menu_entries_ref,
                        &wifi_menu_store_ref,
                        &wifi_menu_cb,
                        &wifi_menu_settings_ref,
                    );
                    rebuild();
                }
            });
        }

        // Subscribe to WiFi network changes
        {
            let entries_ref = entries.clone();
            let store_ref = store.clone();
            let cb = action_callback.clone();
            let settings_available_ref = settings_available.clone();
            store.subscribe_type(entity::network::WIFI_NETWORK_ENTITY_TYPE, move || {
                wifi::update_wifi_menus(&entries_ref, &store_ref, &cb, &settings_available_ref);
            });
        }

        // Subscribe to Ethernet connection profile changes
        {
            let entries_ref = entries.clone();
            let store_ref = store.clone();
            let cb = action_callback.clone();
            let settings_available_ref = settings_available.clone();
            store.subscribe_type(
                entity::network::ETHERNET_CONNECTION_ENTITY_TYPE,
                move || {
                    wired::update_ethernet_menus(
                        &entries_ref,
                        &store_ref,
                        &cb,
                        &settings_available_ref,
                    );
                },
            );
        }

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

                if let Some(entry) = entries_mut.iter().find(|e| e.urn_str == "vpn-consolidated") {
                    // Update existing consolidated toggle
                    entry.toggle.set_active(any_active);
                    entry.toggle.set_busy(any_busy);
                    entry.toggle.set_details(details.clone());
                    entry.toggle.set_expandable(!vpns.is_empty());

                    // Update VPN menu rows
                    vpn::update_vpn_menu_rows(entry, &vpns, &cb);
                } else {
                    // Create consolidated VPN toggle
                    let widget_id = "network-toggle-vpn-consolidated";
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
                    };

                    // Populate VPN menu rows
                    vpn::update_vpn_menu_rows(&entry, &vpns, &cb);

                    entries_mut.push(entry);
                    drop(entries_mut);
                    rebuild();
                }
            });
        }

        // Subscribe to app entity changes (for settings button visibility).
        // Uses a shared reconcile closure for both subscribe_type and initial
        // reconciliation via idle_add_local_once — required because
        // subscribe_type only fires on changes, not for entities already cached.
        {
            let reconcile = {
                let entries_ref = entries.clone();
                let settings_available_ref = settings_available.clone();
                let settings_urn_ref = settings_urn.clone();
                let store_ref = store.clone();

                move || {
                    let apps: Vec<(Urn, entity::app::App)> =
                        store_ref.get_entities_typed(entity::app::ENTITY_TYPE);

                    let settings_app_urn = find_settings_app_urn(&apps);
                    let has_settings = settings_app_urn.is_some();
                    let was_available = settings_available_ref.get();
                    settings_available_ref.set(has_settings);
                    *settings_urn_ref.borrow_mut() = settings_app_urn;

                    if has_settings != was_available {
                        let entries = entries_ref.borrow();
                        for entry in entries.iter() {
                            if let Some(ref btn_container) = entry.settings_button {
                                btn_container.set_visible(has_settings);
                                let has_info = !entry.info_rows.borrow().is_empty();
                                let has_children = !entry.network_rows.borrow().is_empty();
                                entry
                                    .toggle
                                    .set_expandable(has_info || has_children || has_settings);
                            }
                        }
                    }
                }
            };

            store.subscribe_type(entity::app::ENTITY_TYPE, reconcile.clone());
            gtk::glib::idle_add_local_once(reconcile);
        }

        // Subscribe to tethering connection changes
        {
            let entries_ref = entries.clone();
            let store_ref = store.clone();
            let cb = action_callback.clone();
            store.subscribe_type(
                entity::network::TETHERING_CONNECTION_ENTITY_TYPE,
                move || {
                    tethering::update_tethering_menus(&entries_ref, &store_ref, &cb);
                },
            );
        }

        Self {
            entries,
            store: store.clone(),
            action_callback: action_callback.clone(),
            menu_store: menu_store.clone(),
            settings_available,
        }
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
                    toggle: (*entry.toggle).clone(),
                    menu: Some(entry.menu.widget().clone()),
                })
            })
            .collect()
    }
}

/// Determine the icon for a network adapter based on its kind and state.
fn adapter_icon(adapter: &entity::network::NetworkAdapter) -> String {
    match &adapter.kind {
        entity::network::AdapterKind::Wired => {
            if adapter.connected {
                "network-wired-symbolic"
            } else {
                "network-wired-disconnected-symbolic"
            }
        }
        entity::network::AdapterKind::Wireless => {
            if adapter.connected {
                "network-wireless-signal-good-symbolic" // Will be updated by child network data
            } else {
                "network-wireless-offline-symbolic"
            }
        }
        entity::network::AdapterKind::Tethering => "network-cellular-symbolic",
    }
    .to_string()
}

/// Determine the title for a network adapter based on its kind.
fn adapter_title(adapter: &entity::network::NetworkAdapter) -> String {
    match &adapter.kind {
        entity::network::AdapterKind::Wired => crate::i18n::t("network-wired"),
        entity::network::AdapterKind::Wireless => "Wi-Fi".to_string(),
        entity::network::AdapterKind::Tethering => "Tethering".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use waft_protocol::entity;

    fn make_app_entry(plugin: &str, id: &str) -> (Urn, entity::app::App) {
        let urn = Urn::new(plugin, entity::app::ENTITY_TYPE, id);
        let app = entity::app::App {
            name: "Test App".to_string(),
            icon: "test-icon".to_string(),
            available: true,
            keywords: vec![],
            description: None,
        };
        (urn, app)
    }

    #[test]
    fn settings_urn_found_when_internal_apps_present() {
        let apps = vec![make_app_entry("internal-apps", "waft-settings")];
        let expected = Urn::new("internal-apps", entity::app::ENTITY_TYPE, "waft-settings");
        assert_eq!(find_settings_app_urn(&apps), Some(expected));
    }

    #[test]
    fn settings_urn_none_when_only_xdg_apps_present() {
        let apps = vec![
            make_app_entry("xdg-apps", "firefox"),
            make_app_entry("xdg-apps", "nautilus"),
        ];
        assert_eq!(find_settings_app_urn(&apps), None);
    }

    #[test]
    fn settings_urn_found_among_mixed_app_entities() {
        let settings_urn = Urn::new("internal-apps", entity::app::ENTITY_TYPE, "waft-settings");
        let apps = vec![
            make_app_entry("xdg-apps", "firefox"),
            make_app_entry("xdg-apps", "nautilus"),
            (
                settings_urn.clone(),
                entity::app::App {
                    name: "Settings".to_string(),
                    icon: "preferences-system-symbolic".to_string(),
                    available: true,
                    keywords: vec![],
                    description: None,
                },
            ),
        ];
        assert_eq!(find_settings_app_urn(&apps), Some(settings_urn));
    }

    #[test]
    fn settings_urn_none_when_no_apps() {
        assert_eq!(find_settings_app_urn(&[]), None);
    }
}

/// Find the waft-settings app entity URN from a list of app entities.
///
/// Returns `Some(urn)` only for `internal-apps/app/waft-settings`, which is
/// the only entity that handles the `open-page` action for settings navigation.
fn find_settings_app_urn(apps: &[(Urn, entity::app::App)]) -> Option<Urn> {
    apps.iter()
        .find(|(urn, _)| urn.plugin() == "internal-apps" && urn.id() == "waft-settings")
        .map(|(urn, _)| urn.clone())
}

/// Build a settings button container (separator + button) for adapter menus.
///
/// Returns a vertical `gtk::Box` containing a separator and a button row.
/// Visibility is controlled by `settings_available`.
fn build_settings_button(
    settings_urn: &Rc<RefCell<Option<Urn>>>,
    action_callback: &EntityActionCallback,
    page: &str,
    i18n_key: &str,
) -> FeatureToggleMenuSettingsButton {
    let button = FeatureToggleMenuSettingsButton::new(FeatureToggleMenuSettingsButtonProps {
        label: i18n::t(i18n_key),
    });

    let settings_urn_ref = settings_urn.clone();
    let cb = action_callback.clone();
    let page = page.to_string();

    button.on_click(move |_| {
        if let Some(ref urn) = *settings_urn_ref.borrow() {
            cb(
                urn.clone(),
                "open-page".to_string(),
                serde_json::json!({ "page": page }),
            );
        }
    });
    button
}
