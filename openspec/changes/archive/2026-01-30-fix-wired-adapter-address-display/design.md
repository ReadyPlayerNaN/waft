## Context

The wired adapter widget has a menu that displays connection details when expanded. The UI (`EthernetMenuWidget`) already supports all the required fields (IPv4, IPv6, subnet mask, gateway), but the data fetching in `setup_expand_callback` only retrieves link speed via `dbus::get_link_speed()`.

NetworkManager exposes IP configuration through:
- `Ip4Config` property on Device - returns object path to IP4Config
- `Ip6Config` property on Device - returns object path to IP6Config
- IP4Config object has `AddressData` (array of dicts with address/prefix) and `Gateway`
- IP6Config object has `AddressData` (array of dicts with address/prefix) and `Gateway`

The existing codebase uses the established async pattern: spawn thread → create tokio runtime → execute async D-Bus calls → send results via mpsc channel → poll with glib::timeout_add_local.

## Goals / Non-Goals

**Goals:**
- Fetch and display IPv4 address, subnet mask, and gateway for connected wired adapters
- Fetch and display IPv6 address when available
- Follow existing D-Bus access patterns in `dbus.rs`
- Maintain the established async thread + channel pattern

**Non-Goals:**
- Adding MAC address display (not in current `ConnectionDetails` struct)
- Real-time updates while menu is open (current behavior is fetch-on-expand)
- nmrs integration for IP config (nmrs doesn't expose IP4Config/IP6Config)

## Decisions

### 1. Use raw D-Bus for IP config retrieval

**Decision**: Add new functions in `dbus.rs` using raw D-Bus calls via `zbus`.

**Rationale**: The codebase already uses raw D-Bus for `get_link_speed` because nmrs doesn't expose it. IP config is similarly not exposed by nmrs, so following the same pattern keeps the code consistent.

**Alternatives considered**:
- Extend nmrs to add IP config support - too much scope creep for this fix
- Use a different D-Bus library - would add unnecessary dependency

### 2. Create a dedicated `IpConfiguration` struct for D-Bus layer

**Decision**: Return a new `IpConfiguration` struct from the D-Bus functions, then map it to `ConnectionDetails` in the widget layer.

**Rationale**: Separates D-Bus concerns from UI concerns. The D-Bus layer returns raw network data, the widget layer decides how to format and display it.

### 3. Fetch IP config in the existing expand callback

**Decision**: Extend the async block in `setup_expand_callback` to also call the new IP config functions.

**Rationale**: The callback already spawns a thread and creates a tokio runtime. Adding more async calls there is simpler than creating a separate fetch mechanism.

### 4. Convert prefix length to dotted decimal for subnet mask

**Decision**: Convert CIDR prefix (e.g., 24) to dotted decimal (e.g., 255.255.255.0) for display.

**Rationale**: Users expect traditional subnet mask format. The `network-connection-details` spec mentions "255.255.255.0 or /24 notation" - dotted decimal is more familiar to most users.

## Risks / Trade-offs

**Risk**: IP4Config or IP6Config object paths may be "/" when not configured → Check for "/" path and treat as None

**Risk**: AddressData format may vary across NetworkManager versions → Use defensive parsing with fallbacks

**Risk**: Multiple IP addresses on single interface → Display only the first/primary address (matches typical use case)

**Trade-off**: Fetching IP config adds latency to menu expansion → Acceptable since link speed fetch already exists and users expect a brief delay
