## Context

The networkmanager plugin currently manages WiFi and Ethernet adapters with a consistent pattern:
- **Toggle Widget**: Displays connection status, handles activate/deactivate clicks
- **Menu Widget**: Shows detailed info or connection options when expanded
- **Adapter Widget**: Coordinates toggle + menu, handles D-Bus operations, manages store updates

VPN placeholder files exist (`vpn_toggle.rs`, `vpn_menu.rs`) but are empty. The store already supports VPN state management (`VpnState`, `VpnConnectionState`, `NetworkOp::SetVpnConnections`, `NetworkOp::SetVpnState`).

Key constraint: Use direct D-Bus calls, not nmrs library (being phased out).

## Goals / Non-Goals

**Goals:**
- Single expandable VPN toggle showing "VPN" (disconnected) or VPN name (connected)
- Menu listing all configured VPNs with connect/disconnect switches
- Support all VPN states: Disconnected, Connecting, Connected, Disconnecting
- Follow Bluetooth device menu pattern (name + switch, full row clickable)
- Real-time state updates via D-Bus signal subscriptions

**Non-Goals:**
- VPN configuration/setup (users configure VPNs via system settings)
- Support for VPN types requiring interactive authentication during connect
- Multiple simultaneous VPN connections display (show first active only)
- VPN connection details (IP, server, etc.) in expanded menu

## Decisions

### 1. Single VPN Widget vs Per-VPN Widgets

**Decision:** Single `VpnWidget` managing all VPN connections (not per-VPN widgets like WiFi adapters).

**Rationale:** Unlike WiFi/Ethernet where each physical adapter is independent, VPNs are logical connections that share the same toggle behavior. A single widget with ID `networkmanager:vpn` simplifies management and matches user mental model ("VPN" as a feature, not individual adapters).

**Alternatives considered:**
- Per-VPN widgets: Rejected - would clutter the panel with multiple VPN toggles

### 2. Toggle Label Strategy

**Decision:** Show "VPN" when disconnected, active VPN name when connected.

**Rationale:** Matches user request. When multiple VPNs are configured but none active, "VPN" is clearer than listing all options. Connected VPN name provides immediate context.

**Alternatives considered:**
- Always show "VPN": Rejected - loses context of which VPN is active
- Show count "VPN (3 configured)": Rejected - adds noise, count rarely useful

### 3. Toggle Click Behavior

**Decision:**
- Disconnected: Open menu (expand toggle)
- Connected: Disconnect active VPN directly

**Rationale:** Matches user request. When connected, one-click disconnect is convenient. When disconnected, user must choose which VPN to connect.

**Alternatives considered:**
- Always expand menu: Rejected - extra click to disconnect is annoying
- Connect to last-used VPN: Rejected - assumes user intent, could connect to wrong VPN

### 4. Menu Row Structure

**Decision:** Follow Bluetooth device menu pattern exactly:
- VPN icon (left)
- VPN name (center, expanding)
- Spinner (visible during connecting/disconnecting)
- Switch toggle (right)
- Entire row clickable (triggers switch)

**Rationale:** Consistency with existing Bluetooth pattern. Users already understand this interaction.

### 5. D-Bus Integration Approach

**Decision:** Direct zbus D-Bus calls following existing patterns in `dbus.rs`.

**D-Bus interfaces needed:**
- `org.freedesktop.NetworkManager.Settings` - List connection profiles
- `org.freedesktop.NetworkManager` - ActivateConnection, DeactivateConnection
- `org.freedesktop.NetworkManager.Connection.Active` - Monitor active connection state
- `org.freedesktop.NetworkManager.VPN.Connection` - VPN-specific state signals

**Rationale:** nmrs is being phased out. Direct D-Bus matches existing codebase patterns.

### 6. VPN Connection Discovery

**Decision:** Query `org.freedesktop.NetworkManager.Settings.ListConnections()`, filter for `connection.type == "vpn"`.

**Rationale:** NetworkManager stores VPN configs as connection profiles. This gives us all configured VPNs regardless of VPN type (OpenVPN, WireGuard, etc.).

### 7. State Synchronization

**Decision:**
- Initial load: Query active connections, check for VPN type
- Runtime: Subscribe to `StateChanged` signal on active VPN connections
- Store updates via `NetworkOp::SetVpnConnections` and `NetworkOp::SetVpnState`

**Rationale:** Matches WiFi/Ethernet pattern. Store already has VPN operations defined.

## Risks / Trade-offs

**[Risk] VPN activation may require authentication**
- Mitigation: Only activate saved connections with stored credentials. VPNs requiring interactive auth will fail gracefully (show error state).

**[Risk] Multiple active VPNs**
- Mitigation: Display first connected VPN in toggle. Menu shows all with their individual states.

**[Risk] VPN state transitions may be slow**
- Mitigation: Show spinner during Connecting/Disconnecting states. Disable switch interaction during transitions.

**[Trade-off] No VPN details in menu**
- Accepting simpler implementation. Users who need details can use system settings.

**[Trade-off] No "connect to last used" shortcut**
- Keeping behavior explicit. User always chooses which VPN via menu.
