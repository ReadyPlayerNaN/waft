//! Static registry mapping (entity_type, action) to display metadata for the command palette.

use std::collections::HashMap;

use serde::Serialize;

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
    /// URN to use when no live entities exist for this entity type.
    /// `None` means the command only appears when entities are present.
    pub static_urn: Option<&'static str>,
}

/// A resolved command ready for display or execution.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ResolvedCommand {
    pub label: String,
    pub subtitle: Option<String>,
    pub urn: crate::Urn,
    pub action: String,
    pub icon: String,
    pub entity_type: &'static str,
}

fn no_subtitle(_v: &serde_json::Value) -> Option<String> {
    None
}

fn active_subtitle(v: &serde_json::Value) -> Option<String> {
    v.get("active")
        .and_then(serde_json::Value::as_bool)
        .map(|active| {
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
        .map(std::string::ToString::to_string)
}

pub static COMMAND_DEFS: &[CommandDef] = &[
    // Session actions
    CommandDef {
        entity_type: SESSION_ENTITY_TYPE,
        action: "lock",
        label: "Lock Screen",
        icon: "system-lock-screen-symbolic",
        subtitle_fn: no_subtitle,
        static_urn: Some("systemd/session/default"),
    },
    CommandDef {
        entity_type: SESSION_ENTITY_TYPE,
        action: "logout",
        label: "Log Out",
        icon: "system-log-out-symbolic",
        subtitle_fn: no_subtitle,
        static_urn: Some("systemd/session/default"),
    },
    CommandDef {
        entity_type: SESSION_ENTITY_TYPE,
        action: "reboot",
        label: "Reboot",
        icon: "system-reboot-symbolic",
        subtitle_fn: no_subtitle,
        static_urn: Some("systemd/session/default"),
    },
    CommandDef {
        entity_type: SESSION_ENTITY_TYPE,
        action: "shutdown",
        label: "Shut Down",
        icon: "system-shutdown-symbolic",
        subtitle_fn: no_subtitle,
        static_urn: Some("systemd/session/default"),
    },
    CommandDef {
        entity_type: SESSION_ENTITY_TYPE,
        action: "suspend",
        label: "Suspend",
        icon: "weather-clear-night-symbolic",
        subtitle_fn: no_subtitle,
        static_urn: Some("systemd/session/default"),
    },
    // Dark mode
    CommandDef {
        entity_type: DARK_MODE_ENTITY_TYPE,
        action: "toggle",
        label: "Toggle Dark Mode",
        icon: "weather-clear-night-symbolic",
        subtitle_fn: active_subtitle,
        static_urn: Some("darkman/dark-mode/default"),
    },
    // Night light
    CommandDef {
        entity_type: NIGHT_LIGHT_ENTITY_TYPE,
        action: "toggle",
        label: "Toggle Night Light",
        icon: "night-light-symbolic",
        subtitle_fn: active_subtitle,
        static_urn: Some("sunsetr/night-light/default"),
    },
    // Caffeine (sleep inhibitor)
    CommandDef {
        entity_type: SLEEP_INHIBITOR_ENTITY_TYPE,
        action: "toggle",
        label: "Toggle Caffeine",
        icon: "preferences-system-time-symbolic",
        subtitle_fn: active_subtitle,
        static_urn: Some("caffeine/sleep-inhibitor/default"),
    },
    // Do Not Disturb
    CommandDef {
        entity_type: DND_ENTITY_TYPE,
        action: "toggle",
        label: "Toggle Do Not Disturb",
        icon: "notifications-disabled-symbolic",
        subtitle_fn: active_subtitle,
        static_urn: Some("notifications/dnd/default"),
    },
    // Recording
    CommandDef {
        entity_type: RECORDING_ENTITY_TYPE,
        action: "toggle",
        label: "Toggle Recording",
        icon: "media-record-symbolic",
        subtitle_fn: active_subtitle,
        static_urn: Some("notifications/recording/default"),
    },
    // Bluetooth device
    CommandDef {
        entity_type: BluetoothDevice::ENTITY_TYPE,
        action: "connect",
        label: "Connect",
        icon: "bluetooth-symbolic",
        subtitle_fn: name_subtitle,
        static_urn: None,
    },
    CommandDef {
        entity_type: BluetoothDevice::ENTITY_TYPE,
        action: "disconnect",
        label: "Disconnect",
        icon: "bluetooth-disconnected-symbolic",
        subtitle_fn: name_subtitle,
        static_urn: None,
    },
    // VPN
    CommandDef {
        entity_type: VPN_ENTITY_TYPE,
        action: "connect",
        label: "Connect VPN",
        icon: "network-vpn-symbolic",
        subtitle_fn: name_subtitle,
        static_urn: None,
    },
    CommandDef {
        entity_type: VPN_ENTITY_TYPE,
        action: "disconnect",
        label: "Disconnect VPN",
        icon: "network-vpn-symbolic",
        subtitle_fn: name_subtitle,
        static_urn: None,
    },
    // Syncthing (backup method)
    CommandDef {
        entity_type: BACKUP_METHOD_ENTITY_TYPE,
        action: "toggle",
        label: "Toggle Syncthing",
        icon: "folder-sync-symbolic",
        subtitle_fn: active_subtitle,
        static_urn: Some("syncthing/backup-method/default"),
    },
];

/// Returns the unique set of entity types needed for command palette subscriptions.
pub fn resolve_commands(
    entity_map: &HashMap<String, Vec<(crate::Urn, serde_json::Value)>>,
) -> Vec<ResolvedCommand> {
    let mut commands = Vec::new();

    for def in COMMAND_DEFS {
        let maybe_entities = entity_map.get(def.entity_type);

        if maybe_entities.is_none_or(Vec::is_empty) {
            if let Some(raw_urn) = def.static_urn
                && let Ok(urn) = crate::Urn::parse(raw_urn)
            {
                commands.push(ResolvedCommand {
                    label: def.label.to_string(),
                    subtitle: None,
                    urn,
                    action: def.action.to_string(),
                    icon: def.icon.to_string(),
                    entity_type: def.entity_type,
                });
            }
            continue;
        }

        let entities = maybe_entities.expect("checked above");
        for (urn, data) in entities {
            let subtitle = (def.subtitle_fn)(data);
            let label = if entities.len() > 1 {
                match subtitle.as_deref() {
                    Some(name) => format!("{} {}", def.label, name),
                    None => def.label.to_string(),
                }
            } else {
                def.label.to_string()
            };

            commands.push(ResolvedCommand {
                label,
                subtitle,
                urn: urn.clone(),
                action: def.action.to_string(),
                icon: def.icon.to_string(),
                entity_type: def.entity_type,
            });
        }
    }

    commands
}

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolve_commands_uses_static_urn_when_no_entities_exist() {
        let entity_map = HashMap::new();
        let commands = resolve_commands(&entity_map);
        assert!(commands.iter().any(|c| c.label == "Toggle Night Light"));
        assert!(
            commands
                .iter()
                .any(|c| c.urn.as_str() == "sunsetr/night-light/default")
        );
    }

    #[test]
    fn resolve_commands_prefers_live_entities_for_multi_instance_types() {
        let mut entity_map = HashMap::new();
        entity_map.insert(
            VPN_ENTITY_TYPE.to_string(),
            vec![(
                crate::Urn::new("networkmanager", VPN_ENTITY_TYPE, "work"),
                serde_json::json!({"name": "Work VPN"}),
            )],
        );

        let commands = resolve_commands(&entity_map);
        assert!(commands.iter().any(|c| c.label == "Connect VPN"));
        assert!(commands.iter().any(|c| c.label == "Disconnect VPN"));
    }
}
