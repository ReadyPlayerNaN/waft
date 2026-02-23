//! WiFi adapter toggles.
//!
//! Subscribes to `network-adapter` (filtered to `AdapterKind::Wireless`) and
//! `wifi-network` entity types. Creates one FeatureToggleWidget per wireless adapter.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use gtk::prelude::*;
use waft_client::{EntityActionCallback, EntityStore};
use waft_protocol::Urn;
use waft_protocol::entity;
use waft_ui_gtk::icons::IconWidget;
use waft_ui_gtk::menu_state::{menu_id_for_widget, toggle_menu};
use waft_ui_gtk::vdom::Component;
use waft_ui_gtk::widgets::feature_toggle::{FeatureToggleOutput, FeatureToggleProps, FeatureToggleWidget};

use super::network_menu_logic::{details_text, should_be_expandable};
use super::{NetworkRow, ToggleEntry, adapter_icon, adapter_title};
use crate::components::toggles::settings_app_tracker::SettingsAppTracker;
use crate::layout::types::WidgetFeatureToggle;
use crate::ui::feature_toggles::menu::FeatureToggleMenuWidget;
use crate::ui::feature_toggles::menu_settings::FeatureToggleMenuSettingsButtonProps;

/// Dynamic set of toggles for wireless network adapters.
pub struct WifiToggles {
    entries: Rc<RefCell<Vec<ToggleEntry>>>,
    #[allow(dead_code)]
    store: Rc<EntityStore>,
    #[allow(dead_code)]
    action_callback: EntityActionCallback,
    #[allow(dead_code)]
    menu_store: Rc<waft_core::menu_state::MenuStore>,
    #[allow(dead_code)]
    settings_tracker: Rc<SettingsAppTracker>,
}

impl WifiToggles {
    pub fn new(
        store: &Rc<EntityStore>,
        action_callback: &EntityActionCallback,
        menu_store: &Rc<waft_core::menu_state::MenuStore>,
        rebuild_callback: Rc<dyn Fn()>,
    ) -> Self {
        let entries: Rc<RefCell<Vec<ToggleEntry>>> = Rc::new(RefCell::new(Vec::new()));
        let settings_available: Rc<Cell<bool>> = Rc::new(Cell::new(false));

        // Track waft-settings app availability for settings button visibility.
        let settings_tracker: Rc<SettingsAppTracker> = {
            let entries_ref = entries.clone();
            let settings_available_ref = settings_available.clone();
            Rc::new(SettingsAppTracker::new(store, move |is_available| {
                settings_available_ref.set(is_available);
                let entries = entries_ref.borrow();
                for entry in entries.iter() {
                    if let Some(ref btn) = entry.settings_button {
                        if let Some(ref label) = entry.settings_button_label {
                            btn.update(&FeatureToggleMenuSettingsButtonProps {
                                label: label.clone(),
                                visible: is_available,
                            });
                        }
                        let has_children = !entry.network_rows.borrow().is_empty();
                        entry
                            .toggle
                            .set_expandable(has_children || is_available);
                    }
                }
            }))
        };

        // Subscribe to network adapter changes (wireless only)
        {
            let store_ref = store.clone();
            let entries_ref = entries.clone();
            let cb = action_callback.clone();
            let rebuild = rebuild_callback.clone();
            let menu_store_ref = menu_store.clone();
            let settings_tracker_for_adapter = settings_tracker.clone();
            // Extra clones for update_wifi_menus call after entry changes
            let wifi_menu_store_ref = store.clone();
            let wifi_menu_entries_ref = entries.clone();
            let wifi_menu_cb = action_callback.clone();
            let wifi_menu_settings_ref = settings_available.clone();

            store.subscribe_type(entity::network::ADAPTER_ENTITY_TYPE, move || {
                let adapters: Vec<(Urn, entity::network::NetworkAdapter)> =
                    store_ref.get_entities_typed(entity::network::ADAPTER_ENTITY_TYPE);

                // Filter to wireless adapters only
                let adapters: Vec<_> = adapters
                    .into_iter()
                    .filter(|(_, a)| matches!(a.kind, entity::network::AdapterKind::Wireless))
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
                        let widget_id = format!("wifi-toggle-{}", urn_str);
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
                        let menu_id_for_expand = menu_id.clone();
                        let menu_store_for_expand = menu_store_ref.clone();
                        toggle.connect_output(move |output| {
                            match output {
                                FeatureToggleOutput::Activate | FeatureToggleOutput::Deactivate => {
                                    action_cb(
                                        action_urn.clone(),
                                        "activate".to_string(),
                                        serde_json::Value::Null,
                                    );
                                }
                                FeatureToggleOutput::ExpandToggle(_) => {
                                    toggle_menu(&menu_store_for_expand, &menu_id_for_expand);
                                }
                            }
                        });

                        // Create settings button
                        let has_settings = settings_tracker_for_adapter.is_available();
                        let label = crate::i18n::t("wifi-settings-button");
                        let btn = settings_tracker_for_adapter.build_settings_button(
                            &cb,
                            "wifi",
                            label.clone(),
                            has_settings,
                        );
                        menu.append(&btn.widget());

                        let entry = ToggleEntry {
                            urn_str,
                            toggle,
                            menu,
                            network_rows: RefCell::new(Vec::new()),
                            info_rows: RefCell::new(Vec::new()),
                            weight: 150,
                            connected,
                            settings_button: Some(btn),
                            settings_button_label: Some(label),
                        };

                        entries_mut.push(entry);
                        changed = true;
                    }
                }

                if changed {
                    drop(entries_mut);
                    // Re-evaluate WiFi menus immediately so newly-created toggle entries
                    // pick up the correct expandable state based on current has_settings
                    // and any wifi-network entities already in the store.
                    update_wifi_menus(
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
                update_wifi_menus(&entries_ref, &store_ref, &cb, &settings_available_ref);
            });
        }

        Self {
            entries,
            store: store.clone(),
            action_callback: action_callback.clone(),
            menu_store: menu_store.clone(),
            settings_tracker,
        }
    }

    /// Return all current toggles as feature toggle widgets for the grid.
    pub fn as_feature_toggles(&self) -> Vec<Rc<WidgetFeatureToggle>> {
        self.entries
            .borrow()
            .iter()
            .map(|entry| {
                Rc::new(WidgetFeatureToggle {
                    id: format!("wifi-toggle-{}", entry.urn_str),
                    weight: entry.weight,
                    toggle: (*entry.toggle).clone(),
                    menu: Some(entry.menu.widget().clone()),
                })
            })
            .collect()
    }
}

/// Update WiFi network menus for all wireless adapters based on current network entities.
fn update_wifi_menus(
    entries: &Rc<RefCell<Vec<ToggleEntry>>>,
    store: &Rc<EntityStore>,
    action_callback: &EntityActionCallback,
    settings_available: &Rc<Cell<bool>>,
) {
    let networks: Vec<(Urn, entity::network::WiFiNetwork)> =
        store.get_entities_typed(entity::network::WIFI_NETWORK_ENTITY_TYPE);

    let entries_mut = entries.borrow();
    let has_settings = settings_available.get();

    for entry in entries_mut.iter() {
        // Find networks for this adapter by checking URN prefix
        let adapter_urn_prefix = format!("{}/", entry.urn_str);
        let adapter_networks: Vec<_> = networks
            .iter()
            .filter(|(urn, _)| urn.as_str().starts_with(&adapter_urn_prefix))
            .collect();

        // Update toggle expandable state based on network count or settings availability
        let adapter_networks_owned: Vec<_> = adapter_networks
            .iter()
            .map(|(urn, net)| ((*urn).clone(), (*net).clone()))
            .collect();
        entry.toggle.set_expandable(should_be_expandable(
            adapter_networks_owned.len(),
            has_settings,
        ));

        // Update details text
        entry
            .toggle
            .set_details(details_text(&adapter_networks_owned));

        // Update network rows
        let mut network_rows = entry.network_rows.borrow_mut();

        // Remove rows for networks that no longer exist
        let current_network_urns: Vec<String> = adapter_networks
            .iter()
            .map(|(urn, _)| urn.as_str().to_string())
            .collect();
        network_rows.retain(|row| {
            if current_network_urns.iter().any(|u| u == row.urn_str()) {
                true
            } else {
                row.remove_from(entry.menu.root());
                false
            }
        });

        // Update or create rows for each network
        for (network_urn, network) in &adapter_networks {
            let network_urn_str = network_urn.as_str().to_string();

            if network_rows.iter().any(|r| r.urn_str() == network_urn_str) {
                // Network row already exists - no update needed for now
            } else {
                // Create new network row
                let row_box = gtk::Box::builder()
                    .orientation(gtk::Orientation::Horizontal)
                    .spacing(12)
                    .css_classes(["menu-row", "clickable"])
                    .build();

                // Signal strength icon
                let icon_name = match network.strength {
                    s if s > 75 => "network-wireless-signal-excellent-symbolic",
                    s if s > 50 => "network-wireless-signal-good-symbolic",
                    s if s > 25 => "network-wireless-signal-ok-symbolic",
                    _ => "network-wireless-signal-weak-symbolic",
                };
                let icon_widget = IconWidget::from_name(icon_name, 24);
                row_box.append(icon_widget.widget());

                // SSID label
                let ssid_label = gtk::Label::builder()
                    .label(&network.ssid)
                    .hexpand(true)
                    .xalign(0.0)
                    .build();
                row_box.append(&ssid_label);

                // Security icon
                if network.secure {
                    let lock_icon = IconWidget::from_name("channel-secure-symbolic", 24);
                    row_box.append(lock_icon.widget());
                }

                // Connected indicator
                if network.connected {
                    let check_icon = IconWidget::from_name("object-select-symbolic", 24);
                    row_box.append(check_icon.widget());
                }

                // Make row clickable
                let gesture = gtk::GestureClick::new();
                let action_cb = action_callback.clone();
                let urn_for_click = network_urn.clone();
                let is_connected = network.connected;
                gesture.connect_released(move |_, _, _, _| {
                    let action = if is_connected {
                        "disconnect"
                    } else {
                        "connect"
                    };
                    action_cb(
                        urn_for_click.clone(),
                        action.to_string(),
                        serde_json::Value::Null,
                    );
                });
                row_box.add_controller(gesture);

                entry.menu.append(&row_box);

                network_rows.push(NetworkRow::Plain {
                    urn_str: network_urn_str,
                    root: row_box,
                });
            }
        }

        // Re-append settings button to keep it last in the menu
        if let Some(ref btn_container) = entry.settings_button {
            entry
                .menu
                .reorder_child_after(&btn_container.widget(), entry.menu.last_child().as_ref());
        }
    }
}
