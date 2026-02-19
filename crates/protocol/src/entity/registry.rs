//! Static protocol registry of all entity types.
//!
//! This is the **compile-time static registry** providing English-only descriptions
//! maintained centrally in the protocol crate. It describes what the protocol defines
//! (entity type shapes, properties, actions) and is consumed by `waft protocol`.
//!
//! This is DIFFERENT from plugin-provided runtime descriptions (localized, per-plugin)
//! exposed via `waft plugin describe`. Both coexist:
//! - `waft protocol` -> static registry (what the protocol defines)
//! - `waft plugin describe` -> plugin runtime metadata (what the plugin actually provides)

use serde::Serialize;

/// Metadata about a single entity type in the protocol.
#[derive(Debug, Clone, Serialize)]
pub struct EntityTypeInfo {
    /// The entity type string used in Subscribe/EntityUpdated messages (e.g. "audio-device").
    pub entity_type: &'static str,
    /// Domain grouping (e.g. "audio", "display"). Matches the Rust module name.
    pub domain: &'static str,
    /// One-line human-readable description.
    pub description: &'static str,
    /// Example URN pattern (e.g. "{plugin}/audio-device/{id}").
    pub urn_pattern: &'static str,
    /// Struct properties with name, type description, and help text.
    pub properties: &'static [PropertyInfo],
    /// Supported actions with parameter schemas.
    pub actions: &'static [ActionInfo],
}

/// A property (struct field) of an entity type.
#[derive(Debug, Clone, Serialize)]
pub struct PropertyInfo {
    pub name: &'static str,
    pub type_description: &'static str,
    pub description: &'static str,
    pub optional: bool,
}

/// An action supported by an entity type.
#[derive(Debug, Clone, Serialize)]
pub struct ActionInfo {
    pub name: &'static str,
    pub description: &'static str,
    pub params: &'static [ParamInfo],
}

/// A parameter of an action.
#[derive(Debug, Clone, Serialize)]
pub struct ParamInfo {
    pub name: &'static str,
    pub type_description: &'static str,
    pub description: &'static str,
    pub required: bool,
}

/// Returns all entity types known to the protocol, sorted by domain then entity type.
pub fn all_entity_types() -> &'static [EntityTypeInfo] {
    static REGISTRY: &[EntityTypeInfo] = &[
        // ── app ──
        EntityTypeInfo {
            entity_type: super::app::ENTITY_TYPE,
            domain: "app",
            description: "A launchable application",
            urn_pattern: "{plugin}/app/{id}",
            properties: &[
                PropertyInfo { name: "name", type_description: "string", description: "Application display name", optional: false },
                PropertyInfo { name: "icon", type_description: "string", description: "Themed icon name", optional: false },
                PropertyInfo { name: "available", type_description: "bool", description: "Whether the application binary was found", optional: false },
            ],
            actions: &[
                ActionInfo {
                    name: "open",
                    description: "Launch the application",
                    params: &[],
                },
                ActionInfo {
                    name: "open-page",
                    description: "Launch the application at a specific page",
                    params: &[ParamInfo { name: "page", type_description: "string", description: "Page identifier to navigate to", required: true }],
                },
            ],
        },

        // ── audio ──
        EntityTypeInfo {
            entity_type: super::audio::ENTITY_TYPE,
            domain: "audio",
            description: "An audio input or output device",
            urn_pattern: "{plugin}/audio-device/{id}",
            properties: &[
                PropertyInfo { name: "name", type_description: "string", description: "Device display name", optional: false },
                PropertyInfo { name: "icon", type_description: "string", description: "Icon name for the device", optional: false },
                PropertyInfo { name: "connection_icon", type_description: "string", description: "Connection type icon (e.g. bluetooth)", optional: true },
                PropertyInfo { name: "volume", type_description: "float", description: "Volume level (0.0 - 1.0)", optional: false },
                PropertyInfo { name: "muted", type_description: "bool", description: "Whether the device is muted", optional: false },
                PropertyInfo { name: "default", type_description: "bool", description: "Whether this is the default device", optional: false },
                PropertyInfo { name: "kind", type_description: "enum(Output, Input)", description: "Output or input device", optional: false },
            ],
            actions: &[
                ActionInfo {
                    name: "set-volume",
                    description: "Set the device volume",
                    params: &[ParamInfo { name: "volume", type_description: "float", description: "New volume level (0.0 - 1.0)", required: true }],
                },
                ActionInfo {
                    name: "set-muted",
                    description: "Set the mute state",
                    params: &[ParamInfo { name: "muted", type_description: "bool", description: "Whether to mute", required: true }],
                },
                ActionInfo {
                    name: "set-default",
                    description: "Make this the default device",
                    params: &[],
                },
            ],
        },

        // ── bluetooth ──
        EntityTypeInfo {
            entity_type: super::bluetooth::BluetoothAdapter::ENTITY_TYPE,
            domain: "bluetooth",
            description: "A Bluetooth adapter (e.g. hci0)",
            urn_pattern: "{plugin}/bluetooth-adapter/{adapter-id}",
            properties: &[
                PropertyInfo { name: "name", type_description: "string", description: "Adapter display name", optional: false },
                PropertyInfo { name: "powered", type_description: "bool", description: "Whether the adapter is powered on", optional: false },
                PropertyInfo { name: "discoverable", type_description: "bool", description: "Whether the adapter is discoverable", optional: false },
                PropertyInfo { name: "discovering", type_description: "bool", description: "Whether the adapter is scanning", optional: false },
            ],
            actions: &[
                ActionInfo {
                    name: "toggle",
                    description: "Toggle the adapter power state",
                    params: &[],
                },
                ActionInfo {
                    name: "start-discovery",
                    description: "Start scanning for devices",
                    params: &[],
                },
                ActionInfo {
                    name: "stop-discovery",
                    description: "Stop scanning for devices",
                    params: &[],
                },
            ],
        },
        EntityTypeInfo {
            entity_type: super::bluetooth::BluetoothDevice::ENTITY_TYPE,
            domain: "bluetooth",
            description: "A Bluetooth device paired or visible to an adapter",
            urn_pattern: "{plugin}/bluetooth-adapter/{adapter-id}/bluetooth-device/{mac}",
            properties: &[
                PropertyInfo { name: "name", type_description: "string", description: "Device display name", optional: false },
                PropertyInfo { name: "device_type", type_description: "string", description: "Device type (e.g. audio-headphones)", optional: false },
                PropertyInfo { name: "connection_state", type_description: "enum(Disconnected, Connecting, Connected, Disconnecting)", description: "Connection lifecycle state", optional: false },
                PropertyInfo { name: "battery_percentage", type_description: "u8", description: "Battery level (0-100)", optional: true },
                PropertyInfo { name: "paired", type_description: "bool", description: "Whether the device is paired", optional: false },
                PropertyInfo { name: "trusted", type_description: "bool", description: "Whether the device is trusted", optional: false },
                PropertyInfo { name: "rssi", type_description: "i16", description: "Signal strength indicator", optional: true },
            ],
            actions: &[
                ActionInfo { name: "connect", description: "Connect to the device", params: &[] },
                ActionInfo { name: "disconnect", description: "Disconnect from the device", params: &[] },
                ActionInfo { name: "pair", description: "Pair with the device", params: &[] },
                ActionInfo { name: "remove", description: "Remove (unpair) the device", params: &[] },
                ActionInfo { name: "trust", description: "Trust the device", params: &[] },
                ActionInfo { name: "untrust", description: "Remove trust from the device", params: &[] },
            ],
        },

        // ── calendar ──
        EntityTypeInfo {
            entity_type: super::calendar::ENTITY_TYPE,
            domain: "calendar",
            description: "A calendar event from EDS",
            urn_pattern: "{plugin}/calendar-event/{uid}",
            properties: &[
                PropertyInfo { name: "uid", type_description: "string", description: "Unique event identifier", optional: false },
                PropertyInfo { name: "summary", type_description: "string", description: "Event title", optional: false },
                PropertyInfo { name: "start_time", type_description: "i64", description: "Start time as Unix timestamp", optional: false },
                PropertyInfo { name: "end_time", type_description: "i64", description: "End time as Unix timestamp", optional: false },
                PropertyInfo { name: "all_day", type_description: "bool", description: "Whether this is an all-day event", optional: false },
                PropertyInfo { name: "description", type_description: "string", description: "Event description", optional: true },
                PropertyInfo { name: "location", type_description: "string", description: "Event location", optional: true },
                PropertyInfo { name: "attendees", type_description: "array", description: "List of attendees", optional: false },
            ],
            actions: &[],
        },
        EntityTypeInfo {
            entity_type: super::calendar::CALENDAR_SYNC_ENTITY_TYPE,
            domain: "calendar",
            description: "Calendar sync control",
            urn_pattern: "{plugin}/calendar-sync/{id}",
            properties: &[
                PropertyInfo { name: "last_refresh", type_description: "i64", description: "Unix timestamp of last refresh", optional: true },
                PropertyInfo { name: "syncing", type_description: "bool", description: "Whether a sync is in progress", optional: false },
            ],
            actions: &[
                ActionInfo { name: "refresh", description: "Trigger an immediate calendar sync", params: &[] },
            ],
        },

        // ── clock ──
        EntityTypeInfo {
            entity_type: super::clock::ENTITY_TYPE,
            domain: "clock",
            description: "Current time and date",
            urn_pattern: "{plugin}/clock/{id}",
            properties: &[
                PropertyInfo { name: "time", type_description: "string", description: "Formatted time string", optional: false },
                PropertyInfo { name: "date", type_description: "string", description: "Formatted date string", optional: false },
            ],
            actions: &[],
        },

        // ── display ──
        EntityTypeInfo {
            entity_type: super::display::DARK_MODE_ENTITY_TYPE,
            domain: "display",
            description: "Dark mode toggle state",
            urn_pattern: "{plugin}/dark-mode/{id}",
            properties: &[
                PropertyInfo { name: "active", type_description: "bool", description: "Whether dark mode is active", optional: false },
            ],
            actions: &[
                ActionInfo { name: "toggle", description: "Toggle dark mode on/off", params: &[] },
            ],
        },
        EntityTypeInfo {
            entity_type: super::display::DARK_MODE_AUTOMATION_CONFIG_ENTITY_TYPE,
            domain: "display",
            description: "Dark mode automation configuration",
            urn_pattern: "{plugin}/dark-mode-automation-config/{id}",
            properties: &[
                PropertyInfo { name: "latitude", type_description: "f64", description: "Latitude for sun-based switching", optional: true },
                PropertyInfo { name: "longitude", type_description: "f64", description: "Longitude for sun-based switching", optional: true },
                PropertyInfo { name: "auto_location", type_description: "bool", description: "Whether to detect location automatically", optional: true },
                PropertyInfo { name: "dbus_api", type_description: "bool", description: "Whether D-Bus API is enabled", optional: true },
                PropertyInfo { name: "portal_api", type_description: "bool", description: "Whether portal API is enabled", optional: true },
                PropertyInfo { name: "schema", type_description: "object", description: "Field availability and constraints schema", optional: false },
            ],
            actions: &[
                ActionInfo {
                    name: "update",
                    description: "Update automation configuration fields",
                    params: &[],
                },
            ],
        },
        EntityTypeInfo {
            entity_type: super::display::DISPLAY_ENTITY_TYPE,
            domain: "display",
            description: "A display with adjustable brightness",
            urn_pattern: "{plugin}/display/{id}",
            properties: &[
                PropertyInfo { name: "name", type_description: "string", description: "Display name", optional: false },
                PropertyInfo { name: "brightness", type_description: "float", description: "Brightness level (0.0 - 1.0)", optional: false },
                PropertyInfo { name: "kind", type_description: "enum(Backlight, External)", description: "Display backend type", optional: false },
            ],
            actions: &[
                ActionInfo {
                    name: "set-brightness",
                    description: "Set the display brightness",
                    params: &[ParamInfo { name: "brightness", type_description: "float", description: "New brightness level (0.0 - 1.0)", required: true }],
                },
            ],
        },
        EntityTypeInfo {
            entity_type: super::display::DISPLAY_OUTPUT_ENTITY_TYPE,
            domain: "display",
            description: "A display output with resolution and refresh rate",
            urn_pattern: "{plugin}/display-output/{name}",
            properties: &[
                PropertyInfo { name: "name", type_description: "string", description: "Output name (e.g. DP-3, HDMI-1)", optional: false },
                PropertyInfo { name: "make", type_description: "string", description: "Manufacturer name", optional: false },
                PropertyInfo { name: "model", type_description: "string", description: "Model name", optional: false },
                PropertyInfo { name: "current_mode", type_description: "object", description: "Currently active display mode", optional: false },
                PropertyInfo { name: "available_modes", type_description: "array", description: "All available display modes", optional: false },
                PropertyInfo { name: "vrr_supported", type_description: "bool", description: "Whether VRR is supported", optional: false },
                PropertyInfo { name: "vrr_enabled", type_description: "bool", description: "Whether VRR is enabled", optional: false },
            ],
            actions: &[
                ActionInfo {
                    name: "set-mode",
                    description: "Change the display mode",
                    params: &[
                        ParamInfo { name: "width", type_description: "u32", description: "Horizontal resolution", required: true },
                        ParamInfo { name: "height", type_description: "u32", description: "Vertical resolution", required: true },
                        ParamInfo { name: "refresh_rate", type_description: "float", description: "Refresh rate in Hz", required: true },
                    ],
                },
                ActionInfo {
                    name: "set-vrr",
                    description: "Enable or disable variable refresh rate",
                    params: &[ParamInfo { name: "enabled", type_description: "bool", description: "Whether to enable VRR", required: true }],
                },
            ],
        },
        EntityTypeInfo {
            entity_type: super::display::NIGHT_LIGHT_ENTITY_TYPE,
            domain: "display",
            description: "Night light (blue light filter) state",
            urn_pattern: "{plugin}/night-light/{id}",
            properties: &[
                PropertyInfo { name: "active", type_description: "bool", description: "Whether night light is active", optional: false },
                PropertyInfo { name: "period", type_description: "string", description: "Current period (e.g. night, day)", optional: true },
                PropertyInfo { name: "next_transition", type_description: "string", description: "Time of next transition", optional: true },
                PropertyInfo { name: "presets", type_description: "array", description: "Available presets", optional: false },
                PropertyInfo { name: "active_preset", type_description: "string", description: "Currently active preset", optional: true },
            ],
            actions: &[
                ActionInfo { name: "toggle", description: "Toggle night light on/off", params: &[] },
            ],
        },
        EntityTypeInfo {
            entity_type: super::display::NIGHT_LIGHT_CONFIG_ENTITY_TYPE,
            domain: "display",
            description: "Night light configuration",
            urn_pattern: "{plugin}/night-light-config/{id}",
            properties: &[
                PropertyInfo { name: "target", type_description: "string", description: "Target display", optional: false },
                PropertyInfo { name: "backend", type_description: "string", description: "Color backend", optional: false },
                PropertyInfo { name: "transition_mode", type_description: "string", description: "Transition mode (e.g. geo)", optional: false },
                PropertyInfo { name: "night_temp", type_description: "string", description: "Night color temperature", optional: false },
                PropertyInfo { name: "day_temp", type_description: "string", description: "Day color temperature", optional: false },
            ],
            actions: &[
                ActionInfo {
                    name: "update",
                    description: "Update night light configuration fields",
                    params: &[],
                },
            ],
        },
        EntityTypeInfo {
            entity_type: super::display::WALLPAPER_MANAGER_ENTITY_TYPE,
            domain: "display",
            description: "Wallpaper manager for a display output",
            urn_pattern: "{plugin}/wallpaper-manager/{output}",
            properties: &[
                PropertyInfo { name: "output", type_description: "string", description: "Output name or 'all' for sync mode", optional: false },
                PropertyInfo { name: "current_wallpaper", type_description: "string", description: "Absolute path to current wallpaper image", optional: true },
                PropertyInfo { name: "available", type_description: "bool", description: "Whether swww-daemon is running", optional: false },
                PropertyInfo { name: "transition", type_description: "object", description: "Transition animation parameters", optional: false },
                PropertyInfo { name: "wallpaper_dir", type_description: "string", description: "Configured wallpaper directory", optional: false },
                PropertyInfo { name: "sync", type_description: "bool", description: "Whether all outputs are synchronized", optional: false },
            ],
            actions: &[
                ActionInfo {
                    name: "set-wallpaper",
                    description: "Set wallpaper to a specific image file",
                    params: &[ParamInfo { name: "path", type_description: "string", description: "Absolute path to image file", required: true }],
                },
                ActionInfo {
                    name: "random",
                    description: "Set a random wallpaper from the configured directory",
                    params: &[],
                },
                ActionInfo {
                    name: "update-transition",
                    description: "Update transition animation parameters",
                    params: &[
                        ParamInfo { name: "transition_type", type_description: "string", description: "Transition type (fade, wipe, grow, etc.)", required: false },
                        ParamInfo { name: "fps", type_description: "u32", description: "Animation frames per second", required: false },
                        ParamInfo { name: "angle", type_description: "u32", description: "Transition angle in degrees", required: false },
                        ParamInfo { name: "duration", type_description: "f64", description: "Transition duration in seconds", required: false },
                    ],
                },
                ActionInfo {
                    name: "update-config",
                    description: "Update wallpaper configuration",
                    params: &[
                        ParamInfo { name: "wallpaper_dir", type_description: "string", description: "Wallpaper directory path", required: false },
                        ParamInfo { name: "sync", type_description: "bool", description: "Synchronize all outputs", required: false },
                    ],
                },
            ],
        },

        // ── keyboard ──
        EntityTypeInfo {
            entity_type: super::keyboard::ENTITY_TYPE,
            domain: "keyboard",
            description: "Active keyboard layout and alternatives",
            urn_pattern: "{plugin}/keyboard-layout/{id}",
            properties: &[
                PropertyInfo { name: "current", type_description: "string", description: "Currently active layout code", optional: false },
                PropertyInfo { name: "available", type_description: "array", description: "Available layout codes", optional: false },
            ],
            actions: &[
                ActionInfo {
                    name: "switch",
                    description: "Switch to a different keyboard layout",
                    params: &[ParamInfo { name: "layout", type_description: "string", description: "Layout code to switch to", required: true }],
                },
            ],
        },
        EntityTypeInfo {
            entity_type: super::keyboard::CONFIG_ENTITY_TYPE,
            domain: "keyboard",
            description: "Keyboard layout configuration",
            urn_pattern: "{plugin}/keyboard-layout-config/{id}",
            properties: &[
                PropertyInfo { name: "mode", type_description: "string", description: "Configuration mode (editable, external-file, system-default, error)", optional: false },
                PropertyInfo { name: "layouts", type_description: "array", description: "Configured layout codes", optional: false },
                PropertyInfo { name: "layout_names", type_description: "array", description: "Custom layout display names", optional: false },
                PropertyInfo { name: "variant", type_description: "string", description: "XKB variant (comma-separated, parallel to layouts)", optional: true },
                PropertyInfo { name: "options", type_description: "string", description: "XKB options", optional: true },
                PropertyInfo { name: "file_path", type_description: "string", description: "External keymap file path", optional: true },
                PropertyInfo { name: "error_message", type_description: "string", description: "Error description", optional: true },
            ],
            actions: &[
                ActionInfo {
                    name: "add",
                    description: "Add a keyboard layout",
                    params: &[
                        ParamInfo { name: "layout", type_description: "string", description: "XKB layout code", required: true },
                        ParamInfo { name: "name", type_description: "string", description: "Display name", required: false },
                    ],
                },
                ActionInfo {
                    name: "remove",
                    description: "Remove a keyboard layout",
                    params: &[ParamInfo { name: "layout", type_description: "string", description: "XKB layout code", required: true }],
                },
                ActionInfo {
                    name: "reorder",
                    description: "Reorder keyboard layouts",
                    params: &[ParamInfo { name: "layouts", type_description: "array", description: "Ordered layout codes", required: true }],
                },
                ActionInfo {
                    name: "set-variant",
                    description: "Set the XKB variant for a specific layout",
                    params: &[
                        ParamInfo { name: "layout", type_description: "string", description: "XKB layout code", required: true },
                        ParamInfo { name: "variant", type_description: "string", description: "XKB variant name (empty string to clear)", required: true },
                    ],
                },
                ActionInfo {
                    name: "rename",
                    description: "Rename a layout (external-file mode only)",
                    params: &[
                        ParamInfo { name: "layout", type_description: "string", description: "XKB layout code", required: true },
                        ParamInfo { name: "name", type_description: "string", description: "New display name", required: true },
                    ],
                },
            ],
        },

        // ── network ──
        EntityTypeInfo {
            entity_type: super::network::ETHERNET_CONNECTION_ENTITY_TYPE,
            domain: "network",
            description: "An Ethernet connection profile",
            urn_pattern: "{plugin}/network-adapter/{adapter}/ethernet-connection/{uuid}",
            properties: &[
                PropertyInfo { name: "name", type_description: "string", description: "Connection profile name", optional: false },
                PropertyInfo { name: "uuid", type_description: "string", description: "Connection UUID", optional: false },
                PropertyInfo { name: "active", type_description: "bool", description: "Whether this connection is active", optional: false },
            ],
            actions: &[
                ActionInfo { name: "activate", description: "Activate this connection", params: &[] },
                ActionInfo { name: "deactivate", description: "Deactivate this connection", params: &[] },
            ],
        },
        EntityTypeInfo {
            entity_type: super::network::ADAPTER_ENTITY_TYPE,
            domain: "network",
            description: "A network adapter (wired or wireless)",
            urn_pattern: "{plugin}/network-adapter/{id}",
            properties: &[
                PropertyInfo { name: "name", type_description: "string", description: "Adapter name", optional: false },
                PropertyInfo { name: "enabled", type_description: "bool", description: "Whether the adapter is enabled", optional: false },
                PropertyInfo { name: "connected", type_description: "bool", description: "Whether the adapter is connected", optional: false },
                PropertyInfo { name: "ip", type_description: "object", description: "IP address information", optional: true },
                PropertyInfo { name: "public_ip", type_description: "string", description: "Public IP address", optional: true },
                PropertyInfo { name: "kind", type_description: "enum(wired, wireless, tethering)", description: "Adapter type", optional: false },
            ],
            actions: &[
                ActionInfo { name: "toggle", description: "Toggle the adapter on/off", params: &[] },
                ActionInfo { name: "scan", description: "Trigger a network scan (wireless only)", params: &[] },
            ],
        },
        EntityTypeInfo {
            entity_type: super::network::TETHERING_CONNECTION_ENTITY_TYPE,
            domain: "network",
            description: "A tethering connection profile",
            urn_pattern: "{plugin}/network-adapter/tethering/tethering-connection/{uuid}",
            properties: &[
                PropertyInfo { name: "name", type_description: "string", description: "Connection profile name", optional: false },
                PropertyInfo { name: "uuid", type_description: "string", description: "Connection UUID", optional: false },
                PropertyInfo { name: "active", type_description: "bool", description: "Whether this connection is active", optional: false },
            ],
            actions: &[
                ActionInfo { name: "activate", description: "Activate this connection", params: &[] },
                ActionInfo { name: "deactivate", description: "Deactivate this connection", params: &[] },
            ],
        },
        EntityTypeInfo {
            entity_type: super::network::VPN_ENTITY_TYPE,
            domain: "network",
            description: "A VPN connection",
            urn_pattern: "{plugin}/vpn/{id}",
            properties: &[
                PropertyInfo { name: "name", type_description: "string", description: "VPN connection name", optional: false },
                PropertyInfo { name: "state", type_description: "enum(Disconnected, Connecting, Connected, Disconnecting)", description: "Connection state", optional: false },
                PropertyInfo { name: "vpn_type", type_description: "enum(vpn, wireguard)", description: "Connection technology type", optional: false },
            ],
            actions: &[
                ActionInfo { name: "connect", description: "Connect to the VPN", params: &[] },
                ActionInfo { name: "disconnect", description: "Disconnect from the VPN", params: &[] },
            ],
        },
        EntityTypeInfo {
            entity_type: super::network::WIFI_NETWORK_ENTITY_TYPE,
            domain: "network",
            description: "A WiFi network",
            urn_pattern: "{plugin}/network-adapter/{adapter}/wifi-network/{ssid}",
            properties: &[
                PropertyInfo { name: "ssid", type_description: "string", description: "Network name", optional: false },
                PropertyInfo { name: "strength", type_description: "u8", description: "Signal strength (0-100)", optional: false },
                PropertyInfo { name: "secure", type_description: "bool", description: "Whether the network is encrypted", optional: false },
                PropertyInfo { name: "known", type_description: "bool", description: "Whether credentials are saved", optional: false },
                PropertyInfo { name: "connected", type_description: "bool", description: "Whether currently connected", optional: false },
            ],
            actions: &[
                ActionInfo { name: "connect", description: "Connect to this network", params: &[] },
                ActionInfo { name: "disconnect", description: "Disconnect from this network", params: &[] },
                ActionInfo { name: "forget", description: "Remove saved credentials", params: &[] },
            ],
        },

        // ── notification ──
        EntityTypeInfo {
            entity_type: super::notification::DND_ENTITY_TYPE,
            domain: "notification",
            description: "Do Not Disturb state",
            urn_pattern: "{plugin}/dnd/{id}",
            properties: &[
                PropertyInfo { name: "active", type_description: "bool", description: "Whether DND is active", optional: false },
            ],
            actions: &[
                ActionInfo { name: "toggle", description: "Toggle Do Not Disturb", params: &[] },
            ],
        },
        EntityTypeInfo {
            entity_type: super::notification::NOTIFICATION_ENTITY_TYPE,
            domain: "notification",
            description: "A desktop notification",
            urn_pattern: "{plugin}/notification/{id}",
            properties: &[
                PropertyInfo { name: "title", type_description: "string", description: "Notification title", optional: false },
                PropertyInfo { name: "description", type_description: "string", description: "Notification body text", optional: false },
                PropertyInfo { name: "app_name", type_description: "string", description: "Source application name", optional: true },
                PropertyInfo { name: "app_id", type_description: "string", description: "Source application identifier", optional: true },
                PropertyInfo { name: "urgency", type_description: "enum(Low, Normal, Critical)", description: "Urgency level", optional: false },
                PropertyInfo { name: "actions", type_description: "array", description: "Available actions", optional: false },
                PropertyInfo { name: "icon_hints", type_description: "array", description: "Icon hints in priority order", optional: false },
                PropertyInfo { name: "created_at_ms", type_description: "i64", description: "Creation timestamp in milliseconds", optional: false },
                PropertyInfo { name: "resident", type_description: "bool", description: "Whether the notification persists after action", optional: false },
                PropertyInfo { name: "workspace", type_description: "string", description: "Source workspace name", optional: true },
                PropertyInfo { name: "suppress_toast", type_description: "bool", description: "Whether to suppress toast popup", optional: false },
            ],
            actions: &[
                ActionInfo { name: "dismiss", description: "Dismiss the notification", params: &[] },
                ActionInfo {
                    name: "invoke-action",
                    description: "Invoke a notification action",
                    params: &[ParamInfo { name: "key", type_description: "string", description: "Action key to invoke", required: true }],
                },
            ],
        },

        // ── notification_filter ──
        EntityTypeInfo {
            entity_type: super::notification_filter::ACTIVE_PROFILE_ENTITY_TYPE,
            domain: "notification_filter",
            description: "Active notification filtering profile",
            urn_pattern: "{plugin}/active-profile/{id}",
            properties: &[
                PropertyInfo { name: "profile_id", type_description: "string", description: "ID of the active profile", optional: false },
            ],
            actions: &[
                ActionInfo {
                    name: "set",
                    description: "Set the active profile",
                    params: &[ParamInfo { name: "profile_id", type_description: "string", description: "Profile ID to activate", required: true }],
                },
            ],
        },
        EntityTypeInfo {
            entity_type: super::notification_filter::NOTIFICATION_GROUP_ENTITY_TYPE,
            domain: "notification_filter",
            description: "A pattern-based notification group",
            urn_pattern: "{plugin}/notification-group/{id}",
            properties: &[
                PropertyInfo { name: "id", type_description: "string", description: "Group identifier", optional: false },
                PropertyInfo { name: "name", type_description: "string", description: "Group display name", optional: false },
                PropertyInfo { name: "order", type_description: "u32", description: "Sort order", optional: false },
                PropertyInfo { name: "matcher", type_description: "object", description: "Rule combinator with match patterns", optional: false },
            ],
            actions: &[
                ActionInfo { name: "create", description: "Create a new group", params: &[] },
                ActionInfo { name: "update", description: "Update group configuration", params: &[] },
                ActionInfo { name: "delete", description: "Delete the group", params: &[] },
            ],
        },
        EntityTypeInfo {
            entity_type: super::notification_filter::NOTIFICATION_PROFILE_ENTITY_TYPE,
            domain: "notification_filter",
            description: "A notification filtering profile",
            urn_pattern: "{plugin}/notification-profile/{id}",
            properties: &[
                PropertyInfo { name: "id", type_description: "string", description: "Profile identifier", optional: false },
                PropertyInfo { name: "name", type_description: "string", description: "Profile display name", optional: false },
                PropertyInfo { name: "rules", type_description: "object", description: "Group rules (group_id -> rule)", optional: false },
            ],
            actions: &[
                ActionInfo { name: "create", description: "Create a new profile", params: &[] },
                ActionInfo { name: "update", description: "Update profile configuration", params: &[] },
                ActionInfo { name: "delete", description: "Delete the profile", params: &[] },
            ],
        },
        EntityTypeInfo {
            entity_type: super::notification_filter::SOUND_CONFIG_ENTITY_TYPE,
            domain: "notification_filter",
            description: "Notification sound configuration",
            urn_pattern: "{plugin}/sound-config/{id}",
            properties: &[
                PropertyInfo { name: "enabled", type_description: "bool", description: "Whether notification sounds are enabled", optional: false },
                PropertyInfo { name: "default_low", type_description: "string", description: "Sound for low-urgency notifications", optional: false },
                PropertyInfo { name: "default_normal", type_description: "string", description: "Sound for normal-urgency notifications", optional: false },
                PropertyInfo { name: "default_critical", type_description: "string", description: "Sound for critical-urgency notifications", optional: false },
            ],
            actions: &[
                ActionInfo { name: "update", description: "Update sound configuration", params: &[] },
            ],
        },

        // ── notification_sound ──
        EntityTypeInfo {
            entity_type: super::notification_sound::NOTIFICATION_SOUND_ENTITY_TYPE,
            domain: "notification_sound",
            description: "A notification sound file from the gallery",
            urn_pattern: "{plugin}/notification-sound/{id}",
            properties: &[
                PropertyInfo { name: "filename", type_description: "string", description: "Sound filename", optional: false },
                PropertyInfo { name: "reference", type_description: "string", description: "Sound reference for config", optional: false },
                PropertyInfo { name: "size", type_description: "u64", description: "File size in bytes", optional: false },
            ],
            actions: &[
                ActionInfo { name: "add-sound", description: "Add a sound file to the gallery", params: &[
                    ParamInfo { name: "filename", type_description: "string", description: "Sound filename", required: true },
                    ParamInfo { name: "data", type_description: "string", description: "Base64-encoded file data", required: true },
                ]},
                ActionInfo { name: "remove-sound", description: "Remove a sound from the gallery", params: &[] },
                ActionInfo { name: "preview-sound", description: "Play a sound for preview", params: &[
                    ParamInfo { name: "reference", type_description: "string", description: "Sound reference (e.g. sounds/alert.ogg)", required: true },
                ]},
            ],
        },

        // ── plugin ──
        EntityTypeInfo {
            entity_type: super::plugin::ENTITY_TYPE,
            domain: "plugin",
            description: "Lifecycle status of a waft plugin",
            urn_pattern: "waft/plugin-status/{plugin-name}",
            properties: &[
                PropertyInfo { name: "name", type_description: "string", description: "Plugin identifier", optional: false },
                PropertyInfo { name: "state", type_description: "enum(available, running, stopped, failed)", description: "Current lifecycle state", optional: false },
                PropertyInfo { name: "entity_types", type_description: "array", description: "Entity types this plugin provides", optional: false },
            ],
            actions: &[],
        },

        // ── power ──
        EntityTypeInfo {
            entity_type: super::power::ENTITY_TYPE,
            domain: "power",
            description: "A battery device",
            urn_pattern: "{plugin}/battery/{id}",
            properties: &[
                PropertyInfo { name: "present", type_description: "bool", description: "Whether a battery is present", optional: false },
                PropertyInfo { name: "percentage", type_description: "float", description: "Charge percentage (0.0 - 100.0)", optional: false },
                PropertyInfo { name: "state", type_description: "enum(Unknown, Charging, Discharging, Empty, FullyCharged, PendingCharge, PendingDischarge)", description: "Charge state", optional: false },
                PropertyInfo { name: "icon_name", type_description: "string", description: "Battery icon name", optional: false },
                PropertyInfo { name: "time_to_empty", type_description: "i64", description: "Seconds until empty", optional: false },
                PropertyInfo { name: "time_to_full", type_description: "i64", description: "Seconds until full", optional: false },
            ],
            actions: &[],
        },

        // ── session ──
        EntityTypeInfo {
            entity_type: super::session::SESSION_ENTITY_TYPE,
            domain: "session",
            description: "User session information",
            urn_pattern: "{plugin}/session/{id}",
            properties: &[
                PropertyInfo { name: "user_name", type_description: "string", description: "Login user name", optional: true },
                PropertyInfo { name: "screen_name", type_description: "string", description: "Display name", optional: true },
            ],
            actions: &[
                ActionInfo { name: "lock", description: "Lock the session", params: &[] },
                ActionInfo { name: "logout", description: "Log out of the session", params: &[] },
                ActionInfo { name: "reboot", description: "Reboot the system", params: &[] },
                ActionInfo { name: "shutdown", description: "Shut down the system", params: &[] },
                ActionInfo { name: "suspend", description: "Suspend the system", params: &[] },
            ],
        },
        EntityTypeInfo {
            entity_type: super::session::SLEEP_INHIBITOR_ENTITY_TYPE,
            domain: "session",
            description: "A sleep/screensaver inhibitor",
            urn_pattern: "{plugin}/sleep-inhibitor/{id}",
            properties: &[
                PropertyInfo { name: "active", type_description: "bool", description: "Whether sleep inhibition is active", optional: false },
            ],
            actions: &[
                ActionInfo { name: "toggle", description: "Toggle sleep inhibition", params: &[] },
            ],
        },

        // ── storage ──
        EntityTypeInfo {
            entity_type: super::storage::BACKUP_METHOD_ENTITY_TYPE,
            domain: "storage",
            description: "A backup method that can be enabled/disabled",
            urn_pattern: "{plugin}/backup-method/{id}",
            properties: &[
                PropertyInfo { name: "name", type_description: "string", description: "Backup method name", optional: false },
                PropertyInfo { name: "enabled", type_description: "bool", description: "Whether the backup method is enabled", optional: false },
                PropertyInfo { name: "icon", type_description: "string", description: "Icon name", optional: false },
            ],
            actions: &[
                ActionInfo { name: "toggle", description: "Toggle the backup method on/off", params: &[] },
            ],
        },

        // ── weather ──
        EntityTypeInfo {
            entity_type: super::weather::ENTITY_TYPE,
            domain: "weather",
            description: "Current weather conditions",
            urn_pattern: "{plugin}/weather/{id}",
            properties: &[
                PropertyInfo { name: "temperature", type_description: "float", description: "Temperature value", optional: false },
                PropertyInfo { name: "condition", type_description: "enum(Clear, PartlyCloudy, Cloudy, Fog, Drizzle, Rain, FreezingRain, Snow, Thunderstorm)", description: "Weather condition", optional: false },
                PropertyInfo { name: "day", type_description: "bool", description: "Whether it is daytime", optional: false },
            ],
            actions: &[],
        },
    ];

    REGISTRY
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn registry_has_all_entity_types() {
        let registry = all_entity_types();
        let types: HashSet<&str> = registry.iter().map(|e| e.entity_type).collect();

        // Verify all known ENTITY_TYPE constants are in the registry.
        let expected = [
            super::super::app::ENTITY_TYPE,
            super::super::audio::ENTITY_TYPE,
            super::super::bluetooth::BluetoothAdapter::ENTITY_TYPE,
            super::super::bluetooth::BluetoothDevice::ENTITY_TYPE,
            super::super::calendar::ENTITY_TYPE,
            super::super::calendar::CALENDAR_SYNC_ENTITY_TYPE,
            super::super::clock::ENTITY_TYPE,
            super::super::display::DISPLAY_ENTITY_TYPE,
            super::super::display::DISPLAY_OUTPUT_ENTITY_TYPE,
            super::super::display::DARK_MODE_ENTITY_TYPE,
            super::super::display::DARK_MODE_AUTOMATION_CONFIG_ENTITY_TYPE,
            super::super::display::NIGHT_LIGHT_ENTITY_TYPE,
            super::super::display::NIGHT_LIGHT_CONFIG_ENTITY_TYPE,
            super::super::display::WALLPAPER_MANAGER_ENTITY_TYPE,
            super::super::keyboard::ENTITY_TYPE,
            super::super::keyboard::CONFIG_ENTITY_TYPE,
            super::super::network::ADAPTER_ENTITY_TYPE,
            super::super::network::WIFI_NETWORK_ENTITY_TYPE,
            super::super::network::ETHERNET_CONNECTION_ENTITY_TYPE,
            super::super::network::VPN_ENTITY_TYPE,
            super::super::network::TETHERING_CONNECTION_ENTITY_TYPE,
            super::super::notification::NOTIFICATION_ENTITY_TYPE,
            super::super::notification::DND_ENTITY_TYPE,
            super::super::notification_filter::NOTIFICATION_GROUP_ENTITY_TYPE,
            super::super::notification_filter::NOTIFICATION_PROFILE_ENTITY_TYPE,
            super::super::notification_filter::ACTIVE_PROFILE_ENTITY_TYPE,
            super::super::notification_filter::SOUND_CONFIG_ENTITY_TYPE,
            super::super::notification_sound::NOTIFICATION_SOUND_ENTITY_TYPE,
            super::super::plugin::ENTITY_TYPE,
            super::super::power::ENTITY_TYPE,
            super::super::session::SESSION_ENTITY_TYPE,
            super::super::session::SLEEP_INHIBITOR_ENTITY_TYPE,
            super::super::storage::BACKUP_METHOD_ENTITY_TYPE,
            super::super::weather::ENTITY_TYPE,
        ];

        for et in &expected {
            assert!(types.contains(et), "missing entity type in registry: {et}");
        }

        assert_eq!(
            registry.len(),
            expected.len(),
            "registry has {} entries but expected {}",
            registry.len(),
            expected.len(),
        );
    }

    #[test]
    fn no_duplicate_entity_types() {
        let registry = all_entity_types();
        let mut seen = HashSet::new();
        for entry in registry {
            assert!(
                seen.insert(entry.entity_type),
                "duplicate entity type: {}",
                entry.entity_type,
            );
        }
    }

    #[test]
    fn sorted_by_domain_then_entity_type() {
        let registry = all_entity_types();
        for window in registry.windows(2) {
            let a = &window[0];
            let b = &window[1];
            let order = a.domain.cmp(b.domain).then(a.entity_type.cmp(b.entity_type));
            assert!(
                order.is_le(),
                "registry not sorted: ({}, {}) should come before ({}, {})",
                a.domain, a.entity_type, b.domain, b.entity_type,
            );
        }
    }

    #[test]
    fn all_domains_are_valid_module_names() {
        let valid_domains = [
            "app", "audio", "bluetooth", "calendar", "clock", "display",
            "keyboard", "network", "notification", "notification_filter",
            "notification_sound", "plugin", "power", "session", "storage", "weather",
        ];
        let valid_set: HashSet<&str> = valid_domains.iter().copied().collect();

        for entry in all_entity_types() {
            assert!(
                valid_set.contains(entry.domain),
                "unknown domain '{}' for entity type '{}'",
                entry.domain, entry.entity_type,
            );
        }
    }

    #[test]
    fn all_entries_have_descriptions() {
        for entry in all_entity_types() {
            assert!(
                !entry.description.is_empty(),
                "empty description for entity type '{}'",
                entry.entity_type,
            );
        }
    }

    #[test]
    fn all_entries_have_urn_patterns() {
        for entry in all_entity_types() {
            assert!(
                !entry.urn_pattern.is_empty(),
                "empty URN pattern for entity type '{}'",
                entry.entity_type,
            );
            // Most entity types use {plugin} as the first URN segment.
            // plugin-status is a special case: the daemon produces it with
            // a fixed "waft" prefix instead of a plugin name.
            if entry.entity_type != super::super::plugin::ENTITY_TYPE {
                assert!(
                    entry.urn_pattern.contains("{plugin}"),
                    "URN pattern for '{}' should contain {{plugin}}: {}",
                    entry.entity_type, entry.urn_pattern,
                );
            }
        }
    }
}
