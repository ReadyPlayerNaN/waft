//! Network adapter and VPN toggle components.
//!
//! Each network type has its own toggle struct:
//! - `WifiToggles` -- wireless adapters + wifi-network menus
//! - `WiredToggles` -- wired adapters + ethernet-connection profiles + IP info
//! - `VpnToggles` -- consolidated VPN toggle with per-connection rows
//! - `TetheringToggles` -- tethering adapters + tethering-connection rows

mod network_menu_logic;
mod tethering;
mod vpn;
mod wifi;
mod wired;

pub use tethering::TetheringToggles;
pub use vpn::VpnToggles;
pub use wifi::WifiToggles;
pub use wired::WiredToggles;

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use gtk::prelude::*;
use waft_protocol::entity;
use waft_ui_gtk::vdom::Component;
use waft_ui_gtk::widgets::feature_toggle::FeatureToggleWidget;

use crate::ui::feature_toggles::menu::FeatureToggleMenuWidget;
use crate::ui::feature_toggles::menu_info_row::FeatureToggleMenuInfoRow;
use crate::ui::feature_toggles::menu_settings::FeatureToggleMenuSettingsButton;
use waft_ui_gtk::widgets::connection_row::ConnectionRow;

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
    /// Settings button for adapter menus (None for VPN/Tethering).
    settings_button: Option<FeatureToggleMenuSettingsButton>,
    /// Label for the settings button (needed for update() calls).
    settings_button_label: Option<String>,
}

/// A single network row in the menu -- either a plain box (WiFi/Ethernet)
/// or a ConnectionRow widget (VPN/Tethering).
pub(super) enum NetworkRow {
    /// WiFi/Ethernet rows using plain gtk::Box layout.
    Plain { urn_str: String, root: gtk::Box },
    /// VPN/Tethering rows using the extracted ConnectionRow widget.
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
            NetworkRow::Connection { row, .. } => parent.remove(&row.widget()),
        }
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
