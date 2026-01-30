## Why

The wired adapter widget menu shows "Odpojeno" (disconnected) even when connected because it only fetches link speed but never retrieves or displays IP address information. The `EthernetMenuWidget` UI already supports displaying IPv4, IPv6, subnet mask, and gateway fields, but `setup_expand_callback` in `wired_adapter_widget.rs` doesn't populate them.

## What Changes

- Add D-Bus functions to fetch IP configuration from NetworkManager (IP4Config and IP6Config)
- Update `setup_expand_callback` to retrieve and populate all connection details:
  - IPv4 address
  - IPv6 address
  - Subnet mask (prefix length)
  - Default gateway
- The link speed fetch already works and will continue to work

## Capabilities

### New Capabilities

- `wired-ip-config-fetch`: D-Bus functions to retrieve IP4Config and IP6Config properties from a NetworkManager device

### Modified Capabilities

- `network-connection-details`: Update the data flow to actually fetch and populate IP address information for wired connections

## Impact

- `src/features/networkmanager/dbus.rs` - Add new async functions for IP config retrieval
- `src/features/networkmanager/wired_adapter_widget.rs` - Update `setup_expand_callback` to fetch all details
- No breaking changes - existing `ConnectionDetails` struct and `EthernetMenuWidget` already support the fields
