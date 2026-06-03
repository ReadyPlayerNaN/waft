# NetworkManager → nmrs Compatibility Matrix

This matrix maps the current Waft NetworkManager plugin feature surface against the public `nmrs` API and records the migration strategy.

Legend:
- **Full**: public `nmrs` API covers the feature directly
- **Partial**: `nmrs` covers most of it, but Waft-specific glue or fallback is still needed
- **Missing**: public `nmrs` API does not expose the needed behavior cleanly

## Summary

| Feature | Current Waft behavior | nmrs status | Migration strategy |
|---|---|---:|---|
| Device discovery (Wi‑Fi/Ethernet) | Enumerate managed non-virtual devices | Full | Move to `nmrs` |
| Bluetooth device discovery | Enumerate NM Bluetooth devices | Full | Move to `nmrs` |
| BlueZ paired/connected visibility | Gate tethering visibility on actual BlueZ link state | Missing | Keep custom BlueZ monitoring |
| Wi‑Fi list (SSID grouped) | Enumerate visible Wi‑Fi networks | Full | Move to `nmrs` |
| Wi‑Fi AP list (per BSSID) | Access-point detail incl. security flags | Full | Move to `nmrs` |
| Wi‑Fi active SSID/AP | Read currently active Wi‑Fi network | Full | Move to `nmrs` |
| Wi‑Fi scan | Trigger scan, refresh state | Full | Move to `nmrs` |
| Wi‑Fi connect (open/WPA/WPA2/WPA3 PSK) | Connect known/unknown networks | Full | Move to `nmrs` |
| Wi‑Fi connect (WEP, fresh) | Create/connect legacy WEP network | Missing | Keep custom fallback |
| Wi‑Fi connect (Enterprise) | Currently rejected | Full | Can move to `nmrs` later; preserve current behavior unless explicitly expanded |
| Wi‑Fi disconnect | Disconnect specific Wi‑Fi interface | Full | Move to `nmrs` |
| Wi‑Fi radio toggle | Global Wi‑Fi on/off | Full | Move to `nmrs` |
| Wi‑Fi saved profile lookup | Find saved connections by SSID | Partial | Prefer `nmrs` saved profiles; keep small glue |
| Wi‑Fi saved profile settings read | Read autoconnect/metered/DNS/IP method | Full | Move to `nmrs` + raw settings helper where needed |
| Wi‑Fi saved profile update | Update autoconnect/metered/DNS/IP method | Full | Move to `nmrs` |
| Wi‑Fi forget saved network | Delete saved profile(s) | Partial | Use `nmrs` where possible; keep SSID-based custom logic if needed |
| Wi‑Fi PSK retrieval | Read secret for QR share | Missing | Keep custom `GetSecrets` path |
| Wi‑Fi QR share | Build QR string from SSID + secret | Missing | Keep custom |
| Ethernet profile enumeration | Child entities per saved Ethernet profile | Full | Move read-side to `nmrs` |
| Ethernet per-profile activation on chosen adapter | Activate specific saved Ethernet profile UUID on a selected adapter | Missing | Keep custom raw D-Bus |
| Ethernet disconnect on adapter | Disconnect active wired connection for a specific adapter | Partial | Use `nmrs.disconnect(Some(interface))` where possible; keep fallback if needed |
| Simple wired connect | Connect first suitable wired device | Full | Move to `nmrs.connect_wired()` |
| VPN enumeration | Saved VPN profiles and active state | Full | Move to `nmrs` |
| VPN connect by saved profile | Activate selected VPN | Full | Move to `nmrs.connect_vpn_by_uuid/id()` |
| VPN disconnect | Disconnect selected VPN | Full | Move to `nmrs.disconnect_vpn_by_uuid()` |
| VPN richer type metadata | OpenVPN/OpenConnect/strongSwan/etc. | Full | Move to `nmrs`; Waft may choose to expose later |
| Bluetooth/tethering connect | Connect saved Bluetooth tethering target | Partial | Use `nmrs.connect_bluetooth()` with Waft glue |
| Bluetooth/tethering disconnect | Disconnect tethering target | Partial | Use `nmrs` if device can be resolved; keep raw fallback |
| Tethering profile enumeration | Saved Bluetooth tethering profiles | Full | Move read-side to `nmrs` |
| Public IP fetch | External HTTP lookup | Missing | Keep custom |
| Airplane mode / radio state | Global radio state control | Full | Can move to `nmrs` later |
| Signal-based refresh triggers | Waft-owned refresh semantics | Partial | Keep current monitoring semantics, swap internals to `nmrs` |

---

## Detailed matrix

### 1. Device and discovery surface

| Feature | Waft requirement | nmrs status | Evidence | Strategy |
|---|---|---:|---|---|
| Ethernet/Wi‑Fi device discovery | managed, non-virtual, typed devices | Full | `list_devices()` and `list_wifi_devices()` exist | Replace custom read-side discovery with `nmrs`; keep virtual-interface filtering in Waft if needed |
| Bluetooth NM device discovery | enumerate Bluetooth devices with state | Full | `list_bluetooth_devices()` exists | Replace custom NM-side Bluetooth device discovery with `nmrs` |
| BlueZ paired + connected device visibility | show tethering only for actually connected paired devices | Missing | public `nmrs` focuses on Bluetooth devices/connectivity, not Waft’s BlueZ-driven visibility gating | Keep custom `bluez_discovery.rs` / `bluez_signal_monitor.rs` logic |

### 2. Wi‑Fi enumeration and monitoring

| Feature | Waft requirement | nmrs status | Evidence | Strategy |
|---|---|---:|---|---|
| Grouped SSID list | visible Wi‑Fi networks | Full | `list_networks(interface)` | Move to `nmrs` |
| Per-BSSID AP detail | security flags, BSSID, strength, active AP | Full | `list_access_points(interface)` + `AccessPoint` model | Move to `nmrs` |
| Current active SSID/AP | active Wi‑Fi entity population | Full | `current_ssid()`, `current_network()`, `list_access_points()` | Move to `nmrs` |
| Scan trigger | request Wi‑Fi scan | Full | `scan_networks(interface)` / `WifiScope::scan()` | Move to `nmrs` |
| Per-interface Wi‑Fi scoping | specific radio control | Full | `wifi("wlanX")` / `WifiScope` | Move to `nmrs` |
| Monitoring triggers | signal-driven refresh semantics | Partial | `nmrs` has monitoring APIs, but Waft’s refresh semantics are custom | Keep Waft-owned monitors; refresh state via `nmrs` reads |

### 3. Wi‑Fi actions

| Feature | Waft requirement | nmrs status | Evidence | Strategy |
|---|---|---:|---|---|
| Connect open/WPA-PSK/WPA2/WPA3 | known + unknown networks | Full | `connect()`, `connect_to_bssid()`, `WifiSecurity::Open/WpaPsk` | Move to `nmrs` |
| Connect saved network with stored creds | reconnect saved profile | Full | `connect()` handles saved profiles | Move to `nmrs` |
| Fresh WEP creation | legacy WEP connect path | Missing | no public WEP connect API found | Keep raw D-Bus fallback |
| Enterprise connect | currently blocked in Waft | Full | `WifiSecurity::WpaEap`, `Wpa3Eap192bit`, EAP options | Preserve current behavior now; optionally expand later |
| Disconnect | specific Wi‑Fi interface disconnect | Full | `disconnect(Some(interface))` / `WifiScope::disconnect()` | Move to `nmrs` |
| Global Wi‑Fi toggle | wireless enabled/disabled | Full | `set_wireless_enabled(bool)` | Move to `nmrs` |

### 4. Wi‑Fi saved profiles, settings, and secrets

| Feature | Waft requirement | nmrs status | Evidence | Strategy |
|---|---|---:|---|---|
| Enumerate saved profiles | known-network detection | Full | `list_saved_connections()` | Move to `nmrs` |
| Read settings | autoconnect, metered, DNS, IP method | Full | `get_saved_connection()`, `get_saved_connection_raw()` | Move to `nmrs` |
| Update settings | autoconnect, metered, DNS, IP method | Full | `update_saved_connection(uuid, patch)` | Move to `nmrs` |
| Forget profile by UUID | delete known profile | Full | `delete_saved_connection(uuid)` | Move to `nmrs` |
| Forget by SSID semantics | delete all profiles for SSID | Partial | `nmrs` is UUID/profile oriented | Keep thin Waft glue for SSID-oriented behavior |
| Read PSK / secrets | QR generation needs secret | Missing | `SavedConnection` docs explicitly exclude secrets | Keep custom `GetSecrets` path |
| Share via QR | return QR payload | Missing | app-specific feature, depends on secrets | Keep custom |

### 5. Ethernet

| Feature | Waft requirement | nmrs status | Evidence | Strategy |
|---|---|---:|---|---|
| Read saved Ethernet profiles | child entities | Full | saved profile APIs cover `802-3-ethernet` | Move read-side to `nmrs` |
| Activate specific Ethernet profile on specific adapter | child action semantics | Missing | no public per-profile/per-adapter Ethernet activation API located | Keep raw D-Bus |
| Disconnect wired adapter | adapter action | Partial | `disconnect(Some(interface))` works at interface level | Use `nmrs` where possible; keep raw fallback if needed |
| Simple wired connect | connect best/default wired device | Full | `connect_wired()` | Use `nmrs` |

### 6. VPN

| Feature | Waft requirement | nmrs status | Evidence | Strategy |
|---|---|---:|---|---|
| Enumerate VPN profiles | list VPN entities | Full | `list_vpn_connections()` | Move to `nmrs` |
| Track active VPN state | connected/connecting/disconnecting | Full | `list_vpn_connections()`, `active_vpn_connections()` | Move to `nmrs` |
| Connect VPN | chosen VPN entity | Full | `connect_vpn_by_uuid()`, `connect_vpn_by_id()` | Move to `nmrs` |
| Disconnect VPN | chosen VPN entity | Full | `disconnect_vpn_by_uuid()` | Move to `nmrs` |
| Richer VPN types | OpenVPN/OpenConnect/strongSwan/etc. | Full | public `VpnType` variants | Optional future enhancement |

### 7. Bluetooth tethering

| Feature | Waft requirement | nmrs status | Evidence | Strategy |
|---|---|---:|---|---|
| Enumerate tethering profiles | Bluetooth child entities | Full | saved profile APIs cover `bluetooth` profiles | Move read-side to `nmrs` |
| Connect tethering profile | activate Bluetooth PAN/DUN | Partial | `connect_bluetooth(name, identity)` exists, but Waft must map profile → device identity | Hybrid: use `nmrs` + Waft glue |
| Disconnect tethering | deactivate Bluetooth PAN/DUN | Partial | no clean high-level “disconnect tethering profile by UUID” API | Hybrid: use `nmrs` where interface is resolvable, keep raw fallback |
| Tethering adapter visibility | show only when paired device connected | Missing | Waft uses BlueZ actual link state | Keep custom BlueZ logic |

### 8. Other Waft-specific behavior

| Feature | Waft requirement | nmrs status | Strategy |
|---|---|---:|---|
| Public IP lookup | shared external IP shown on adapter | Missing | Keep custom |
| Waft entity/state shaping | protocol-specific entity graph | Missing | Keep custom |
| Internal cached UI-oriented state | current plugin behavior | Partial | Keep where helpful; reduce as migration stabilizes |

---

## Recommended hybrid boundary

### Offload to `nmrs`
- device discovery
- Wi‑Fi enumeration and scan
- Wi‑Fi connect/disconnect/toggle
- saved-profile reads and most updates
- VPN enumerate/connect/disconnect
- Bluetooth connect support
- radio/airplane mode APIs where adopted

### Keep custom in Waft
- Ethernet per-profile activation on specific adapter
- Wi‑Fi PSK/secrets retrieval and QR share
- fresh WEP creation/connect fallback
- BlueZ paired/connected tethering visibility heuristic
- public IP fetch
- Waft entity translation/state shaping

---

## Decision guidance

This matrix supports a **hybrid migration** rather than a purity rewrite. The most important non-negotiable custom area is Ethernet child-profile activation. The cleanest strategy is:

1. maximize `nmrs` usage for read-side and common actions
2. keep a small, explicitly justified custom D-Bus island for the missing features
3. document those custom islands in the phase plans and code comments
