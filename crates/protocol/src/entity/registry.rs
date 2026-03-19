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
    const fn prop(
        name: &'static str,
        type_description: &'static str,
        description: &'static str,
    ) -> PropertyInfo {
        PropertyInfo { name, type_description, description, optional: false }
    }

    const fn opt_prop(
        name: &'static str,
        type_description: &'static str,
        description: &'static str,
    ) -> PropertyInfo {
        PropertyInfo { name, type_description, description, optional: true }
    }

    const fn action(name: &'static str, description: &'static str) -> ActionInfo {
        ActionInfo { name, description, params: &[] }
    }

    const fn action_p(
        name: &'static str,
        description: &'static str,
        params: &'static [ParamInfo],
    ) -> ActionInfo {
        ActionInfo { name, description, params }
    }

    const fn param(
        name: &'static str,
        type_description: &'static str,
        description: &'static str,
    ) -> ParamInfo {
        ParamInfo { name, type_description, description, required: false }
    }

    const fn req_param(
        name: &'static str,
        type_description: &'static str,
        description: &'static str,
    ) -> ParamInfo {
        ParamInfo { name, type_description, description, required: true }
    }

    static REGISTRY: &[EntityTypeInfo] = &[
        // ── accounts ──
        EntityTypeInfo {
            entity_type: super::accounts::ONLINE_ACCOUNT_ENTITY_TYPE,
            domain: "accounts",
            description: "A GNOME Online Account with per-service toggles",
            urn_pattern: "{plugin}/online-account/{id}",
            properties: &[
                prop("id", "string", "GOA account ID"),
                prop("provider_name", "string", "Provider display name (e.g. Google, Nextcloud)"),
                prop("presentation_identity", "string", "User-facing account identity (e.g. user@gmail.com)"),
                prop("status", "enum(Active, CredentialsNeeded, NeedsAttention)", "Account health status"),
                prop("services", "Vec<ServiceInfo>", "Per-service enabled/disabled state"),
                prop("locked", "bool", "Whether the account is administrator-locked"),
            ],
            actions: &[
                action_p("enable-service", "Enable a specific service on this account", &[
                    req_param("service_name", "string", "Service to enable"),
                ]),
                action_p("disable-service", "Disable a specific service on this account", &[
                    req_param("service_name", "string", "Service to disable"),
                ]),
                action("remove-account", "Remove this online account"),
            ],
        },
        EntityTypeInfo {
            entity_type: super::accounts::ONLINE_ACCOUNT_PROVIDER_ENTITY_TYPE,
            domain: "accounts",
            description: "An available online account provider",
            urn_pattern: "{plugin}/online-account-provider/{provider-type}",
            properties: &[
                prop("provider_type", "string", "Provider type identifier (e.g. google, ms365)"),
                prop("provider_name", "string", "Human-readable display name"),
                opt_prop("icon_name", "string", "Themed icon name for the provider"),
            ],
            actions: &[
                action("add-account", "Launch the add-account flow for this provider"),
            ],
        },

        // ── ai ──
        EntityTypeInfo {
            entity_type: super::ai::ENTITY_TYPE,
            domain: "ai",
            description: "Claude Code rate limit utilization across time windows",
            urn_pattern: "{plugin}/claude-usage/{id}",
            properties: &[
                prop("five_hour_utilization", "f64", "5-hour window utilization (0.0 - 1.0)"),
                prop("five_hour_reset_at", "i64", "Unix timestamp (ms) when the 5-hour window resets"),
                prop("seven_day_utilization", "f64", "7-day window utilization (0.0 - 1.0)"),
                prop("seven_day_reset_at", "i64", "Unix timestamp (ms) when the 7-day window resets"),
            ],
            actions: &[],
        },

        // ── app ──
        EntityTypeInfo {
            entity_type: super::app::ENTITY_TYPE,
            domain: "app",
            description: "A launchable application",
            urn_pattern: "{plugin}/app/{id}",
            properties: &[
                prop("name", "string", "Application display name"),
                prop("icon", "string", "Themed icon name"),
                prop("available", "bool", "Whether the application binary was found"),
            ],
            actions: &[
                action("open", "Launch the application"),
                action_p("open-page", "Launch the application at a specific page", &[
                    req_param("page", "string", "Page identifier to navigate to"),
                ]),
            ],
        },

        // ── audio ──
        EntityTypeInfo {
            entity_type: super::audio::CARD_ENTITY_TYPE,
            domain: "audio",
            description: "A physical audio card grouping sinks and sources with profile switching",
            urn_pattern: "{plugin}/audio-card/{card-name}",
            properties: &[
                prop("name", "string", "Device display name"),
                opt_prop("device_type", "string", "Semantic device type (e.g. headset, card, display)"),
                opt_prop("connection_type", "string", "Semantic connection type (e.g. bluetooth, jack, hdmi)"),
                prop("active_profile", "string", "Currently active profile name"),
                prop("profiles", "array", "Available card profiles"),
                prop("sinks", "array", "Output sinks belonging to this card"),
                prop("sources", "array", "Input sources belonging to this card (excludes monitors)"),
            ],
            actions: &[
                action_p("set-profile", "Set the card's active profile", &[
                    req_param("profile", "string", "Profile name to activate"),
                ]),
                action_p("set-volume", "Set volume on a sink or source", &[
                    param("sink", "string", "Sink name (for output volume)"),
                    param("source", "string", "Source name (for input volume)"),
                    req_param("value", "float", "Volume level (0.0 - 1.0)"),
                ]),
                action_p("toggle-mute", "Toggle mute on a sink or source", &[
                    param("sink", "string", "Sink name (for output)"),
                    param("source", "string", "Source name (for input)"),
                ]),
                action_p("set-default", "Set a sink or source as the default device", &[
                    param("sink", "string", "Sink name (for output)"),
                    param("source", "string", "Source name (for input)"),
                ]),
                action_p("set-sink-port", "Change the active port on a sink", &[
                    req_param("sink", "string", "Sink name"),
                    req_param("port", "string", "Port name to activate"),
                ]),
                action_p("set-source-port", "Change the active port on a source", &[
                    req_param("source", "string", "Source name"),
                    req_param("port", "string", "Port name to activate"),
                ]),
            ],
        },
        EntityTypeInfo {
            entity_type: super::audio::ENTITY_TYPE,
            domain: "audio",
            description: "An audio input or output device",
            urn_pattern: "{plugin}/audio-device/{id}",
            properties: &[
                prop("name", "string", "Device display name"),
                opt_prop("device_type", "string", "Semantic device type (e.g. headset, card, display)"),
                opt_prop("connection_type", "string", "Semantic connection type (e.g. bluetooth, jack, hdmi)"),
                prop("volume", "float", "Volume level (0.0 - 1.0)"),
                prop("muted", "bool", "Whether the device is muted"),
                prop("default", "bool", "Whether this is the default device"),
                prop("kind", "enum(Output, Input)", "Output or input device"),
                prop("virtual_device", "bool", "Whether this is a waft-managed virtual device"),
                opt_prop("sink_name", "string", "Internal pactl sink or source name (for virtual device actions)"),
            ],
            actions: &[
                action_p("set-volume", "Set the device volume", &[
                    req_param("volume", "float", "New volume level (0.0 - 1.0)"),
                ]),
                action_p("set-muted", "Set the mute state", &[
                    req_param("muted", "bool", "Whether to mute"),
                ]),
                action("set-default", "Make this the default device"),
                action_p("create-sink", "Create a virtual null-sink output device", &[
                    req_param("sink_name", "string", "Internal sink name (waft_ prefixed)"),
                    req_param("label", "string", "Human-readable display name"),
                ]),
                action_p("remove-sink", "Remove a virtual null-sink output device", &[
                    req_param("sink_name", "string", "Internal sink name to remove"),
                ]),
                action_p("create-source", "Create a virtual null-source input device", &[
                    req_param("source_name", "string", "Internal source name (waft_ prefixed)"),
                    req_param("label", "string", "Human-readable display name"),
                ]),
                action_p("remove-source", "Remove a virtual null-source input device", &[
                    req_param("source_name", "string", "Internal source name to remove"),
                ]),
            ],
        },

        // ── bluetooth ──
        EntityTypeInfo {
            entity_type: super::bluetooth::BluetoothAdapter::ENTITY_TYPE,
            domain: "bluetooth",
            description: "A Bluetooth adapter (e.g. hci0)",
            urn_pattern: "{plugin}/bluetooth-adapter/{adapter-id}",
            properties: &[
                prop("name", "string", "Adapter display name"),
                prop("powered", "bool", "Whether the adapter is powered on"),
                prop("discoverable", "bool", "Whether the adapter is discoverable"),
                prop("discovering", "bool", "Whether the adapter is scanning"),
            ],
            actions: &[
                action("toggle", "Toggle the adapter power state"),
                action("start-discovery", "Start scanning for devices"),
                action("stop-discovery", "Stop scanning for devices"),
            ],
        },
        EntityTypeInfo {
            entity_type: super::bluetooth::BluetoothDevice::ENTITY_TYPE,
            domain: "bluetooth",
            description: "A Bluetooth device paired or visible to an adapter",
            urn_pattern: "{plugin}/bluetooth-adapter/{adapter-id}/bluetooth-device/{mac}",
            properties: &[
                prop("name", "string", "Device display name"),
                prop("device_type", "string", "Device type (e.g. audio-headphones)"),
                prop("connection_state", "enum(Disconnected, Connecting, Connected, Disconnecting)", "Connection lifecycle state"),
                opt_prop("battery_percentage", "u8", "Battery level (0-100)"),
                prop("paired", "bool", "Whether the device is paired"),
                prop("trusted", "bool", "Whether the device is trusted"),
                opt_prop("rssi", "i16", "Signal strength indicator"),
            ],
            actions: &[
                action("connect", "Connect to the device"),
                action("disconnect", "Disconnect from the device"),
                action("pair", "Pair with the device"),
                action("remove", "Remove (unpair) the device"),
                action("trust", "Trust the device"),
                action("untrust", "Remove trust from the device"),
            ],
        },

        // ── calendar ──
        EntityTypeInfo {
            entity_type: super::calendar::ENTITY_TYPE,
            domain: "calendar",
            description: "A calendar event from EDS",
            urn_pattern: "{plugin}/calendar-event/{uid}",
            properties: &[
                prop("uid", "string", "Unique event identifier"),
                prop("summary", "string", "Event title"),
                prop("start_time", "i64", "Start time as Unix timestamp"),
                prop("end_time", "i64", "End time as Unix timestamp"),
                prop("all_day", "bool", "Whether this is an all-day event"),
                opt_prop("description", "string", "Event description"),
                opt_prop("location", "string", "Event location"),
                prop("attendees", "array", "List of attendees"),
            ],
            actions: &[],
        },
        EntityTypeInfo {
            entity_type: super::calendar::CALENDAR_SYNC_ENTITY_TYPE,
            domain: "calendar",
            description: "Calendar sync control",
            urn_pattern: "{plugin}/calendar-sync/{id}",
            properties: &[
                opt_prop("last_refresh", "i64", "Unix timestamp of last refresh"),
                prop("syncing", "bool", "Whether a sync is in progress"),
            ],
            actions: &[
                action("refresh", "Trigger an immediate calendar sync"),
            ],
        },

        // ── clock ──
        EntityTypeInfo {
            entity_type: super::clock::ENTITY_TYPE,
            domain: "clock",
            description: "Current time and date",
            urn_pattern: "{plugin}/clock/{id}",
            properties: &[
                prop("time", "string", "Formatted time string"),
                prop("date", "string", "Formatted date string"),
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
                prop("active", "bool", "Whether dark mode is active"),
            ],
            actions: &[
                action("toggle", "Toggle dark mode on/off"),
            ],
        },
        EntityTypeInfo {
            entity_type: super::display::DARK_MODE_AUTOMATION_CONFIG_ENTITY_TYPE,
            domain: "display",
            description: "Dark mode automation configuration",
            urn_pattern: "{plugin}/dark-mode-automation-config/{id}",
            properties: &[
                opt_prop("latitude", "f64", "Latitude for sun-based switching"),
                opt_prop("longitude", "f64", "Longitude for sun-based switching"),
                opt_prop("auto_location", "bool", "Whether to detect location automatically"),
                opt_prop("dbus_api", "bool", "Whether D-Bus API is enabled"),
                opt_prop("portal_api", "bool", "Whether portal API is enabled"),
                prop("schema", "object", "Field availability and constraints schema"),
            ],
            actions: &[
                action("update", "Update automation configuration fields"),
            ],
        },
        EntityTypeInfo {
            entity_type: super::display::DISPLAY_ENTITY_TYPE,
            domain: "display",
            description: "A display with adjustable brightness",
            urn_pattern: "{plugin}/display/{id}",
            properties: &[
                prop("name", "string", "Display name"),
                prop("brightness", "float", "Brightness level (0.0 - 1.0)"),
                prop("kind", "enum(Backlight, External)", "Display backend type"),
                opt_prop("connector", "string", "Compositor output connector (e.g. DP-3, eDP-1)"),
            ],
            actions: &[
                action_p("set-brightness", "Set the display brightness", &[
                    req_param("brightness", "float", "New brightness level (0.0 - 1.0)"),
                ]),
            ],
        },
        EntityTypeInfo {
            entity_type: super::display::DISPLAY_OUTPUT_ENTITY_TYPE,
            domain: "display",
            description: "A display output with resolution and refresh rate",
            urn_pattern: "{plugin}/display-output/{name}",
            properties: &[
                prop("name", "string", "Output name (e.g. DP-3, HDMI-1)"),
                prop("make", "string", "Manufacturer name"),
                prop("model", "string", "Model name"),
                prop("current_mode", "object", "Currently active display mode"),
                prop("available_modes", "array", "All available display modes"),
                prop("vrr_supported", "bool", "Whether VRR is supported"),
                prop("vrr_enabled", "bool", "Whether VRR is enabled"),
            ],
            actions: &[
                action_p("set-mode", "Change the display mode", &[
                    req_param("width", "u32", "Horizontal resolution"),
                    req_param("height", "u32", "Vertical resolution"),
                    req_param("refresh_rate", "float", "Refresh rate in Hz"),
                ]),
                action_p("set-vrr", "Enable or disable variable refresh rate", &[
                    req_param("enabled", "bool", "Whether to enable VRR"),
                ]),
            ],
        },
        EntityTypeInfo {
            entity_type: super::appearance::GTK_APPEARANCE_ENTITY_TYPE,
            domain: "display",
            description: "GTK appearance settings including accent colour",
            urn_pattern: "{plugin}/gtk-appearance/{id}",
            properties: &[
                prop("accent_color", "string", "Current accent colour (blue, teal, green, yellow, orange, red, pink, purple, slate)"),
            ],
            actions: &[
                action_p("set-accent-color", "Set the system accent colour", &[
                    req_param("color", "string", "Accent colour name (blue, teal, green, yellow, orange, red, pink, purple, slate)"),
                ]),
            ],
        },
        EntityTypeInfo {
            entity_type: super::display::NIGHT_LIGHT_ENTITY_TYPE,
            domain: "display",
            description: "Night light (blue light filter) state",
            urn_pattern: "{plugin}/night-light/{id}",
            properties: &[
                prop("active", "bool", "Whether night light is active"),
                opt_prop("period", "string", "Current period (e.g. night, day)"),
                opt_prop("next_transition", "string", "Time of next transition"),
                prop("presets", "array", "Available presets"),
                opt_prop("active_preset", "string", "Currently active preset"),
            ],
            actions: &[
                action("toggle", "Toggle night light on/off"),
            ],
        },
        EntityTypeInfo {
            entity_type: super::display::NIGHT_LIGHT_CONFIG_ENTITY_TYPE,
            domain: "display",
            description: "Night light configuration",
            urn_pattern: "{plugin}/night-light-config/{id}",
            properties: &[
                prop("target", "string", "Target display"),
                prop("backend", "string", "Color backend"),
                prop("transition_mode", "string", "Transition mode (e.g. geo)"),
                prop("night_temp", "string", "Night color temperature"),
                prop("day_temp", "string", "Day color temperature"),
            ],
            actions: &[
                action("update", "Update night light configuration fields"),
            ],
        },
        EntityTypeInfo {
            entity_type: super::display::WALLPAPER_MANAGER_ENTITY_TYPE,
            domain: "display",
            description: "Wallpaper manager for a display output",
            urn_pattern: "{plugin}/wallpaper-manager/{output}",
            properties: &[
                prop("output", "string", "Output name or 'all' for sync mode"),
                opt_prop("current_wallpaper", "string", "Absolute path to current wallpaper image"),
                prop("available", "bool", "Whether swww-daemon is running"),
                prop("transition", "object", "Transition animation parameters"),
                prop("wallpaper_dir", "string", "Configured wallpaper directory"),
                prop("sync", "bool", "Whether all outputs are synchronized"),
            ],
            actions: &[
                action_p("set-wallpaper", "Set wallpaper to a specific image file", &[
                    req_param("path", "string", "Absolute path to image file"),
                ]),
                action("random", "Set a random wallpaper from the configured directory"),
                action_p("update-transition", "Update transition animation parameters", &[
                    param("transition_type", "string", "Transition type (fade, wipe, grow, etc.)"),
                    param("fps", "u32", "Animation frames per second"),
                    param("angle", "u32", "Transition angle in degrees"),
                    param("duration", "f64", "Transition duration in seconds"),
                ]),
                action_p("update-config", "Update wallpaper configuration", &[
                    param("wallpaper_dir", "string", "Wallpaper directory path"),
                    param("sync", "bool", "Synchronize all outputs"),
                ]),
            ],
        },

        // ── keyboard ──
        EntityTypeInfo {
            entity_type: super::keyboard::ENTITY_TYPE,
            domain: "keyboard",
            description: "Active keyboard layout and alternatives",
            urn_pattern: "{plugin}/keyboard-layout/{id}",
            properties: &[
                prop("current", "string", "Currently active layout code"),
                prop("available", "array", "Available layout codes"),
            ],
            actions: &[
                action_p("switch", "Switch to a different keyboard layout", &[
                    req_param("layout", "string", "Layout code to switch to"),
                ]),
            ],
        },
        EntityTypeInfo {
            entity_type: super::keyboard::CONFIG_ENTITY_TYPE,
            domain: "keyboard",
            description: "Keyboard layout configuration",
            urn_pattern: "{plugin}/keyboard-layout-config/{id}",
            properties: &[
                prop("mode", "string", "Configuration mode (editable, external-file, system-default, error)"),
                prop("layouts", "array", "Configured layout codes"),
                prop("layout_names", "array", "Custom layout display names"),
                opt_prop("variant", "string", "XKB variant (comma-separated, parallel to layouts)"),
                opt_prop("options", "string", "XKB options"),
                opt_prop("file_path", "string", "External keymap file path"),
                opt_prop("error_message", "string", "Error description"),
            ],
            actions: &[
                action_p("add", "Add a keyboard layout", &[
                    req_param("layout", "string", "XKB layout code"),
                    param("name", "string", "Display name"),
                ]),
                action_p("remove", "Remove a keyboard layout", &[
                    req_param("layout", "string", "XKB layout code"),
                ]),
                action_p("reorder", "Reorder keyboard layouts", &[
                    req_param("layouts", "array", "Ordered layout codes"),
                ]),
                action_p("set-variant", "Set the XKB variant for a specific layout", &[
                    req_param("layout", "string", "XKB layout code"),
                    req_param("variant", "string", "XKB variant name (empty string to clear)"),
                ]),
                action_p("rename", "Rename a layout (external-file mode only)", &[
                    req_param("layout", "string", "XKB layout code"),
                    req_param("name", "string", "New display name"),
                ]),
            ],
        },

        // ── network ──
        EntityTypeInfo {
            entity_type: super::network::ETHERNET_CONNECTION_ENTITY_TYPE,
            domain: "network",
            description: "An Ethernet connection profile",
            urn_pattern: "{plugin}/network-adapter/{adapter}/ethernet-connection/{uuid}",
            properties: &[
                prop("name", "string", "Connection profile name"),
                prop("uuid", "string", "Connection UUID"),
                prop("active", "bool", "Whether this connection is active"),
            ],
            actions: &[
                action("activate", "Activate this connection"),
                action("deactivate", "Deactivate this connection"),
            ],
        },
        EntityTypeInfo {
            entity_type: super::network::ADAPTER_ENTITY_TYPE,
            domain: "network",
            description: "A network adapter (wired or wireless)",
            urn_pattern: "{plugin}/network-adapter/{id}",
            properties: &[
                prop("name", "string", "Adapter name"),
                prop("enabled", "bool", "Whether the adapter is enabled"),
                prop("connected", "bool", "Whether the adapter is connected"),
                opt_prop("ip", "object", "IP address information"),
                opt_prop("public_ip", "string", "Public IP address"),
                prop("kind", "enum(wired, wireless, tethering)", "Adapter type"),
            ],
            actions: &[
                action("toggle", "Toggle the adapter on/off"),
                action("scan", "Trigger a network scan (wireless only)"),
            ],
        },
        EntityTypeInfo {
            entity_type: super::network::TETHERING_CONNECTION_ENTITY_TYPE,
            domain: "network",
            description: "A tethering connection profile",
            urn_pattern: "{plugin}/network-adapter/tethering/tethering-connection/{uuid}",
            properties: &[
                prop("name", "string", "Connection profile name"),
                prop("uuid", "string", "Connection UUID"),
                prop("active", "bool", "Whether this connection is active"),
            ],
            actions: &[
                action("activate", "Activate this connection"),
                action("deactivate", "Deactivate this connection"),
            ],
        },
        EntityTypeInfo {
            entity_type: super::network::VPN_ENTITY_TYPE,
            domain: "network",
            description: "A VPN connection",
            urn_pattern: "{plugin}/vpn/{id}",
            properties: &[
                prop("name", "string", "VPN connection name"),
                prop("state", "enum(Disconnected, Connecting, Connected, Disconnecting)", "Connection state"),
                prop("vpn_type", "enum(vpn, wireguard)", "Connection technology type"),
            ],
            actions: &[
                action("connect", "Connect to the VPN"),
                action("disconnect", "Disconnect from the VPN"),
            ],
        },
        EntityTypeInfo {
            entity_type: super::network::WIFI_NETWORK_ENTITY_TYPE,
            domain: "network",
            description: "A WiFi network",
            urn_pattern: "{plugin}/network-adapter/{adapter}/wifi-network/{ssid}",
            properties: &[
                prop("ssid", "string", "Network name"),
                prop("strength", "u8", "Signal strength (0-100)"),
                prop("secure", "bool", "Whether the network is encrypted"),
                prop("known", "bool", "Whether credentials are saved"),
                prop("connected", "bool", "Whether currently connected"),
                prop("security_type", "enum(open, wep, wpa, wpa2, wpa3, enterprise)", "Encryption type"),
                prop("connecting", "bool", "Whether a connection attempt is in progress"),
                opt_prop("autoconnect", "bool", "Whether to connect automatically when in range"),
                opt_prop("metered", "enum(Unknown, Yes, No, GuessYes, GuessNo)", "Metered connection state"),
                opt_prop("dns_servers", "Vec<string>", "Configured DNS servers"),
                opt_prop("ip_method", "enum(Auto, Manual, LinkLocal, Disabled)", "IP address configuration method"),
            ],
            actions: &[
                action("connect", "Connect to this network. Params: {password?: string}"),
                action("disconnect", "Disconnect from this network"),
                action("forget", "Remove saved credentials"),
                action_p("update-settings", "Update connection settings", &[
                    param("autoconnect", "bool", "Whether to connect automatically"),
                    param("metered", "enum(Unknown, Yes, No, GuessYes, GuessNo)", "Metered connection state"),
                    param("dns_servers", "Vec<string>", "DNS servers to use"),
                    param("ip_method", "enum(Auto, Manual, LinkLocal, Disabled)", "IP configuration method"),
                ]),
            ],
        },

        // ── notification ──
        EntityTypeInfo {
            entity_type: super::notification::DND_ENTITY_TYPE,
            domain: "notification",
            description: "Do Not Disturb state",
            urn_pattern: "{plugin}/dnd/{id}",
            properties: &[
                prop("active", "bool", "Whether DND is active"),
            ],
            actions: &[
                action("toggle", "Toggle Do Not Disturb"),
            ],
        },
        EntityTypeInfo {
            entity_type: super::notification::NOTIFICATION_ENTITY_TYPE,
            domain: "notification",
            description: "A desktop notification",
            urn_pattern: "{plugin}/notification/{id}",
            properties: &[
                prop("title", "string", "Notification title"),
                prop("description", "string", "Notification body text"),
                opt_prop("app_name", "string", "Source application name"),
                opt_prop("app_id", "string", "Source application identifier"),
                prop("urgency", "enum(Low, Normal, Critical)", "Urgency level"),
                prop("actions", "array", "Available actions"),
                prop("icon_hints", "array", "Icon hints in priority order"),
                prop("created_at_ms", "i64", "Creation timestamp in milliseconds"),
                prop("resident", "bool", "Whether the notification persists after action"),
                opt_prop("workspace", "string", "Source workspace name"),
                prop("suppress_toast", "bool", "Whether to suppress toast popup"),
            ],
            actions: &[
                action("dismiss", "Dismiss the notification"),
                action_p("invoke-action", "Invoke a notification action", &[
                    req_param("key", "string", "Action key to invoke"),
                ]),
            ],
        },

        EntityTypeInfo {
            entity_type: super::notification::RECORDING_ENTITY_TYPE,
            domain: "notification",
            description: "Notification recording state for debugging",
            urn_pattern: "{plugin}/recording/{id}",
            properties: &[
                prop("active", "bool", "Whether recording is active"),
            ],
            actions: &[
                action("toggle", "Toggle notification recording on/off"),
            ],
        },

        // ── notification_filter ──
        EntityTypeInfo {
            entity_type: super::notification_filter::ACTIVE_PROFILE_ENTITY_TYPE,
            domain: "notification_filter",
            description: "Active notification filtering profile",
            urn_pattern: "{plugin}/active-profile/{id}",
            properties: &[
                prop("profile_id", "string", "ID of the active profile"),
            ],
            actions: &[
                action_p("set", "Set the active profile", &[
                    req_param("profile_id", "string", "Profile ID to activate"),
                ]),
            ],
        },
        EntityTypeInfo {
            entity_type: super::notification_filter::NOTIFICATION_GROUP_ENTITY_TYPE,
            domain: "notification_filter",
            description: "A pattern-based notification group",
            urn_pattern: "{plugin}/notification-group/{id}",
            properties: &[
                prop("id", "string", "Group identifier"),
                prop("name", "string", "Group display name"),
                prop("order", "u32", "Sort order"),
                prop("matcher", "object", "Rule combinator with match patterns"),
            ],
            actions: &[
                action("create", "Create a new group"),
                action("update", "Update group configuration"),
                action("delete", "Delete the group"),
            ],
        },
        EntityTypeInfo {
            entity_type: super::notification_filter::NOTIFICATION_PROFILE_ENTITY_TYPE,
            domain: "notification_filter",
            description: "A notification filtering profile",
            urn_pattern: "{plugin}/notification-profile/{id}",
            properties: &[
                prop("id", "string", "Profile identifier"),
                prop("name", "string", "Profile display name"),
                prop("rules", "object", "Group rules (group_id -> rule)"),
            ],
            actions: &[
                action("create", "Create a new profile"),
                action("update", "Update profile configuration"),
                action("delete", "Delete the profile"),
            ],
        },
        EntityTypeInfo {
            entity_type: super::notification_filter::SOUND_CONFIG_ENTITY_TYPE,
            domain: "notification_filter",
            description: "Notification sound configuration",
            urn_pattern: "{plugin}/sound-config/{id}",
            properties: &[
                prop("enabled", "bool", "Whether notification sounds are enabled"),
                prop("default_low", "string", "Sound for low-urgency notifications"),
                prop("default_normal", "string", "Sound for normal-urgency notifications"),
                prop("default_critical", "string", "Sound for critical-urgency notifications"),
            ],
            actions: &[
                action("update", "Update sound configuration"),
            ],
        },

        // ── notification_sound ──
        EntityTypeInfo {
            entity_type: super::notification_sound::NOTIFICATION_SOUND_ENTITY_TYPE,
            domain: "notification_sound",
            description: "A notification sound file from the gallery",
            urn_pattern: "{plugin}/notification-sound/{id}",
            properties: &[
                prop("filename", "string", "Sound filename"),
                prop("reference", "string", "Sound reference for config"),
                prop("size", "u64", "File size in bytes"),
            ],
            actions: &[
                action_p("add-sound", "Add a sound file to the gallery", &[
                    req_param("filename", "string", "Sound filename"),
                    req_param("data", "string", "Base64-encoded file data"),
                ]),
                action("remove-sound", "Remove a sound from the gallery"),
                action_p("preview-sound", "Play a sound for preview", &[
                    req_param("reference", "string", "Sound reference (e.g. sounds/alert.ogg)"),
                ]),
            ],
        },

        // ── plugin ──
        EntityTypeInfo {
            entity_type: super::plugin::ENTITY_TYPE,
            domain: "plugin",
            description: "Lifecycle status of a waft plugin",
            urn_pattern: "waft/plugin-status/{plugin-name}",
            properties: &[
                prop("name", "string", "Plugin identifier"),
                prop("state", "enum(available, running, stopped, failed)", "Current lifecycle state"),
                prop("entity_types", "array", "Entity types this plugin provides"),
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
                prop("present", "bool", "Whether a battery is present"),
                prop("percentage", "float", "Charge percentage (0.0 - 100.0)"),
                prop("state", "enum(Unknown, Charging, Discharging, Empty, FullyCharged, PendingCharge, PendingDischarge)", "Charge state"),
                prop("icon_name", "string", "Battery icon name"),
                prop("time_to_empty", "i64", "Seconds until empty"),
                prop("time_to_full", "i64", "Seconds until full"),
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
                opt_prop("user_name", "string", "Login user name"),
                opt_prop("screen_name", "string", "Display name"),
            ],
            actions: &[
                action("lock", "Lock the session"),
                action("logout", "Log out of the session"),
                action("reboot", "Reboot the system"),
                action("shutdown", "Shut down the system"),
                action("suspend", "Suspend the system"),
            ],
        },
        EntityTypeInfo {
            entity_type: super::session::SLEEP_INHIBITOR_ENTITY_TYPE,
            domain: "session",
            description: "A sleep/screensaver inhibitor",
            urn_pattern: "{plugin}/sleep-inhibitor/{id}",
            properties: &[
                prop("active", "bool", "Whether sleep inhibition is active"),
            ],
            actions: &[
                action("toggle", "Toggle sleep inhibition"),
            ],
        },

        EntityTypeInfo {
            entity_type: super::session::USER_SERVICE_ENTITY_TYPE,
            domain: "session",
            description: "A user-level systemd service",
            urn_pattern: "{plugin}/user-service/{unit-name}",
            properties: &[
                prop("unit", "string", "Full unit name (e.g. pipewire.service)"),
                prop("description", "string", "Human-readable description from unit file"),
                prop("active_state", "string", "Current state (active/inactive/activating/deactivating/failed)"),
                prop("enabled", "bool", "Whether the unit starts on login"),
                prop("sub_state", "string", "Detailed sub-state (e.g. running/dead/exited)"),
            ],
            actions: &[
                action("start", "Start the service"),
                action("stop", "Stop the service"),
                action("enable", "Enable the service on login"),
                action("disable", "Disable the service on login"),
            ],
        },

        EntityTypeInfo {
            entity_type: super::session::USER_TIMER_ENTITY_TYPE,
            domain: "session",
            description: "A user-level systemd timer with its associated service",
            urn_pattern: "{plugin}/user-timer/{unit-name}",
            properties: &[
                prop("name", "string", "Full timer unit name (e.g. backup.timer)"),
                prop("description", "string", "Human-readable description from unit file"),
                prop("enabled", "bool", "Whether the timer is enabled"),
                prop("active", "bool", "Whether the timer is currently active"),
                prop("schedule", "ScheduleKind", "Timer schedule (calendar or relative)"),
                opt_prop("last_trigger", "i64", "Unix timestamp of last trigger (seconds)"),
                opt_prop("next_elapse", "i64", "Unix timestamp of next scheduled trigger (seconds)"),
                opt_prop("last_exit_code", "i32", "Exit code of the last service run"),
                prop("command", "string", "Command executed by the associated service"),
                opt_prop("working_directory", "string", "Working directory for the service"),
                prop("environment", "Vec<(string, string)>", "Environment variables for the service"),
                prop("after", "Vec<string>", "Units this timer's service orders itself after"),
                prop("restart", "enum(no, on-failure, always)", "Restart policy for the service"),
                opt_prop("cpu_quota", "string", "CPU quota (e.g. 50%)"),
                opt_prop("memory_limit", "string", "Memory limit (e.g. 512M)"),
            ],
            actions: &[
                action("enable", "Enable the timer"),
                action("disable", "Disable the timer"),
                action("start", "Start the timer"),
                action("stop", "Stop the timer"),
                action("delete", "Delete the timer and its associated service"),
                action_p("create", "Create a new user timer", &[
                    req_param("name", "string", "Timer unit name"),
                    req_param("command", "string", "Command to execute"),
                    req_param("schedule", "ScheduleKind", "Timer schedule"),
                    param("description", "string", "Human-readable description"),
                    param("working_directory", "string", "Working directory"),
                    param("environment", "Vec<(string, string)>", "Environment variables"),
                    param("after", "Vec<string>", "Units to order after"),
                    param("restart", "enum(no, on-failure, always)", "Restart policy"),
                    param("cpu_quota", "string", "CPU quota"),
                    param("memory_limit", "string", "Memory limit"),
                ]),
                action_p("update", "Update an existing user timer", &[
                    param("command", "string", "New command to execute"),
                    param("schedule", "ScheduleKind", "New timer schedule"),
                    param("description", "string", "New description"),
                    param("working_directory", "string", "New working directory"),
                    param("environment", "Vec<(string, string)>", "New environment variables"),
                    param("after", "Vec<string>", "New units to order after"),
                    param("restart", "enum(no, on-failure, always)", "New restart policy"),
                    param("cpu_quota", "string", "New CPU quota"),
                    param("memory_limit", "string", "New memory limit"),
                ]),
            ],
        },

        // ── storage ──
        EntityTypeInfo {
            entity_type: super::storage::BACKUP_METHOD_ENTITY_TYPE,
            domain: "storage",
            description: "A backup method that can be enabled/disabled",
            urn_pattern: "{plugin}/backup-method/{id}",
            properties: &[
                prop("name", "string", "Backup method name"),
                prop("enabled", "bool", "Whether the backup method is enabled"),
                prop("icon", "string", "Icon name"),
            ],
            actions: &[
                action("toggle", "Toggle the backup method on/off"),
            ],
        },

        // ── weather ──
        EntityTypeInfo {
            entity_type: super::weather::ENTITY_TYPE,
            domain: "weather",
            description: "Current weather conditions",
            urn_pattern: "{plugin}/weather/{id}",
            properties: &[
                prop("temperature", "float", "Temperature value"),
                prop("condition", "enum(Clear, PartlyCloudy, Cloudy, Fog, Drizzle, Rain, FreezingRain, Snow, Thunderstorm)", "Weather condition"),
                prop("day", "bool", "Whether it is daytime"),
            ],
            actions: &[],
        },

        // ── window ──
        EntityTypeInfo {
            entity_type: super::window::ENTITY_TYPE,
            domain: "window",
            description: "An open window in the compositor",
            urn_pattern: "{plugin}/window/{window-id}",
            properties: &[
                prop("title", "string", "Window title"),
                prop("app_id", "string", "Wayland application ID"),
                prop("workspace_id", "number", "Compositor workspace ID"),
                prop("focused", "bool", "Whether the window has keyboard focus"),
            ],
            actions: &[
                action("focus", "Focus the window and bring it to the center"),
            ],
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
            super::super::accounts::ONLINE_ACCOUNT_ENTITY_TYPE,
            super::super::accounts::ONLINE_ACCOUNT_PROVIDER_ENTITY_TYPE,
            super::super::ai::ENTITY_TYPE,
            super::super::app::ENTITY_TYPE,
            super::super::audio::CARD_ENTITY_TYPE,
            super::super::audio::ENTITY_TYPE,
            super::super::bluetooth::BluetoothAdapter::ENTITY_TYPE,
            super::super::bluetooth::BluetoothDevice::ENTITY_TYPE,
            super::super::calendar::ENTITY_TYPE,
            super::super::calendar::CALENDAR_SYNC_ENTITY_TYPE,
            super::super::clock::ENTITY_TYPE,
            super::super::appearance::GTK_APPEARANCE_ENTITY_TYPE,
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
            super::super::notification::RECORDING_ENTITY_TYPE,
            super::super::notification_filter::NOTIFICATION_GROUP_ENTITY_TYPE,
            super::super::notification_filter::NOTIFICATION_PROFILE_ENTITY_TYPE,
            super::super::notification_filter::ACTIVE_PROFILE_ENTITY_TYPE,
            super::super::notification_filter::SOUND_CONFIG_ENTITY_TYPE,
            super::super::notification_sound::NOTIFICATION_SOUND_ENTITY_TYPE,
            super::super::plugin::ENTITY_TYPE,
            super::super::power::ENTITY_TYPE,
            super::super::session::SESSION_ENTITY_TYPE,
            super::super::session::SLEEP_INHIBITOR_ENTITY_TYPE,
            super::super::session::USER_SERVICE_ENTITY_TYPE,
            super::super::session::USER_TIMER_ENTITY_TYPE,
            super::super::storage::BACKUP_METHOD_ENTITY_TYPE,
            super::super::weather::ENTITY_TYPE,
            super::super::window::ENTITY_TYPE,
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
            "accounts", "ai", "app", "audio", "bluetooth", "calendar", "clock", "display",
            "keyboard", "network", "notification", "notification_filter",
            "notification_sound", "plugin", "power", "session", "storage", "weather",
            "window",
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
    fn registry_has_expected_entity_count() {
        let count = all_entity_types().len();
        assert!(count >= 35, "Expected >= 35 entity types, got {count}");
    }

    #[test]
    fn all_entities_have_required_fields() {
        for entry in all_entity_types() {
            assert!(!entry.entity_type.is_empty(), "empty entity_type");
            assert!(!entry.domain.is_empty(), "empty domain in {}", entry.entity_type);
            assert!(!entry.description.is_empty(), "empty description in {}", entry.entity_type);
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
