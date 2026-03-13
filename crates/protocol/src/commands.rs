//! Static registry mapping (entity_type, action) to display metadata for the command palette.

use crate::entity::{
    bluetooth::BluetoothDevice,
    display::{DARK_MODE_ENTITY_TYPE, NIGHT_LIGHT_ENTITY_TYPE},
    network::VPN_ENTITY_TYPE,
    notification::{DND_ENTITY_TYPE, RECORDING_ENTITY_TYPE},
    session::{SESSION_ENTITY_TYPE, SLEEP_INHIBITOR_ENTITY_TYPE},
    storage::BACKUP_METHOD_ENTITY_TYPE,
};

/// A compile-time command definition mapping an entity action to display metadata.
pub struct CommandDef {
    pub entity_type: &'static str,
    pub action: &'static str,
    pub label: &'static str,
    pub icon: &'static str,
    pub subtitle_fn: fn(&serde_json::Value) -> Option<String>,
}

fn no_subtitle(_v: &serde_json::Value) -> Option<String> {
    None
}

fn active_subtitle(v: &serde_json::Value) -> Option<String> {
    v.get("active").and_then(|a| a.as_bool()).map(|active| {
        if active {
            "Active".to_string()
        } else {
            "Inactive".to_string()
        }
    })
}

fn name_subtitle(v: &serde_json::Value) -> Option<String> {
    v.get("name")
        .and_then(|n| n.as_str())
        .map(|s| s.to_string())
}

pub static COMMAND_DEFS: &[CommandDef] = &[
    // Session actions
    CommandDef {
        entity_type: SESSION_ENTITY_TYPE,
        action: "lock",
        label: "Lock Screen",
        icon: "system-lock-screen-symbolic",
        subtitle_fn: no_subtitle,
    },
    CommandDef {
        entity_type: SESSION_ENTITY_TYPE,
        action: "logout",
        label: "Log Out",
        icon: "system-log-out-symbolic",
        subtitle_fn: no_subtitle,
    },
    CommandDef {
        entity_type: SESSION_ENTITY_TYPE,
        action: "reboot",
        label: "Reboot",
        icon: "system-reboot-symbolic",
        subtitle_fn: no_subtitle,
    },
    CommandDef {
        entity_type: SESSION_ENTITY_TYPE,
        action: "shutdown",
        label: "Shut Down",
        icon: "system-shutdown-symbolic",
        subtitle_fn: no_subtitle,
    },
    CommandDef {
        entity_type: SESSION_ENTITY_TYPE,
        action: "suspend",
        label: "Suspend",
        icon: "weather-clear-night-symbolic",
        subtitle_fn: no_subtitle,
    },
    // Dark mode
    CommandDef {
        entity_type: DARK_MODE_ENTITY_TYPE,
        action: "toggle",
        label: "Toggle Dark Mode",
        icon: "weather-clear-night-symbolic",
        subtitle_fn: active_subtitle,
    },
    // Night light
    CommandDef {
        entity_type: NIGHT_LIGHT_ENTITY_TYPE,
        action: "toggle",
        label: "Toggle Night Light",
        icon: "night-light-symbolic",
        subtitle_fn: active_subtitle,
    },
    // Caffeine (sleep inhibitor)
    CommandDef {
        entity_type: SLEEP_INHIBITOR_ENTITY_TYPE,
        action: "toggle",
        label: "Toggle Caffeine",
        icon: "preferences-system-time-symbolic",
        subtitle_fn: active_subtitle,
    },
    // Do Not Disturb
    CommandDef {
        entity_type: DND_ENTITY_TYPE,
        action: "toggle",
        label: "Toggle Do Not Disturb",
        icon: "notifications-disabled-symbolic",
        subtitle_fn: active_subtitle,
    },
    // Recording
    CommandDef {
        entity_type: RECORDING_ENTITY_TYPE,
        action: "toggle",
        label: "Toggle Recording",
        icon: "media-record-symbolic",
        subtitle_fn: active_subtitle,
    },
    // Bluetooth device
    CommandDef {
        entity_type: BluetoothDevice::ENTITY_TYPE,
        action: "connect",
        label: "Connect",
        icon: "bluetooth-symbolic",
        subtitle_fn: name_subtitle,
    },
    CommandDef {
        entity_type: BluetoothDevice::ENTITY_TYPE,
        action: "disconnect",
        label: "Disconnect",
        icon: "bluetooth-disconnected-symbolic",
        subtitle_fn: name_subtitle,
    },
    // VPN
    CommandDef {
        entity_type: VPN_ENTITY_TYPE,
        action: "connect",
        label: "Connect VPN",
        icon: "network-vpn-symbolic",
        subtitle_fn: name_subtitle,
    },
    CommandDef {
        entity_type: VPN_ENTITY_TYPE,
        action: "disconnect",
        label: "Disconnect VPN",
        icon: "network-vpn-symbolic",
        subtitle_fn: name_subtitle,
    },
    // Syncthing (backup method)
    CommandDef {
        entity_type: BACKUP_METHOD_ENTITY_TYPE,
        action: "toggle",
        label: "Toggle Syncthing",
        icon: "folder-sync-symbolic",
        subtitle_fn: active_subtitle,
    },
];

/// Returns the unique set of entity types needed for command palette subscriptions.
pub fn command_entity_types() -> &'static [&'static str] {
    &[
        SESSION_ENTITY_TYPE,
        DARK_MODE_ENTITY_TYPE,
        NIGHT_LIGHT_ENTITY_TYPE,
        SLEEP_INHIBITOR_ENTITY_TYPE,
        DND_ENTITY_TYPE,
        RECORDING_ENTITY_TYPE,
        BluetoothDevice::ENTITY_TYPE,
        VPN_ENTITY_TYPE,
        BACKUP_METHOD_ENTITY_TYPE,
    ]
}
