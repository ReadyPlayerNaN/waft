## Why

The networkmanager plugin currently uses a custom D-Bus parsing implementation (`dbus.rs`) that manually constructs D-Bus method calls and handles property parsing. This duplicates functionality already provided by the `nmrs` crate, which is a mature Rust library specifically designed for NetworkManager D-Bus interactions.

**This migration uses nmrs as the primary API** - the custom D-Bus code in `dbus.rs` is largely replaced with `nmrs`, reducing the codebase by **~450 lines** (from 772 to 318 lines).

Benefits:
- **Less code to maintain** - Reduced from 772 to 318 lines
- **Better reliability** - Battle-tested library vs custom implementation
- **Type safety** - Proper Rust types (`DeviceState`, `AccessPoint`, etc.) instead of raw u32/string parsing
- **Easier feature additions** - Adding new NetworkManager features becomes trivial with nmrs's comprehensive API

## What Changes

- **Remove** custom D-Bus implementation in `src/features/networkmanager/dbus.rs` (~600 lines)
- **Replace** with `nmrs` crate calls and thin adapter layer (~100 lines)
- **Update** `src/features/networkmanager/mod.rs` to initialize and use `nmrs::NetworkManager`
- **Add** `nmrs = "2.0"` dependency to `Cargo.toml`
- **Remove** `DbusHandle` dependency from networkmanager plugin (other plugins continue using it)
- **Adapt** existing NetworkManager operations (device enumeration, connection management, WiFi scanning, etc.) to use `nmrs` equivalents

## Capabilities

### New Capabilities
- `nmrs-integration`: Integration with the `nmrs` crate for NetworkManager D-Bus communication, replacing custom D-Bus implementation

### Modified Capabilities

### Removed Capabilities
- **IP configuration display** - IPv4/IPv6 address, subnet, and gateway display removed from ethernet menu (nmrs does not expose these details directly; link speed is retained)

## Impact

**Code:**
- `src/features/networkmanager/dbus.rs` - Significant refactoring or removal, replaced with `nmrs` API calls
- `src/features/networkmanager/mod.rs` - Update to use `nmrs` types instead of custom `DeviceInfo`, `AccessPoint`, etc.
- `src/features/networkmanager/store.rs` - May need updates if data structures change
- `src/features/networkmanager/ethernet_menu.rs` - May need updates for connection details extraction
- `src/features/networkmanager/wifi_menu.rs` - May need updates for access point handling

**Dependencies:**
- Add `nmrs = "2.0"` crate to `Cargo.toml`
- Remove `DbusHandle` dependency from networkmanager plugin (nmrs manages its own D-Bus connection)
- Remove direct `zbus` usage in networkmanager feature

**APIs:**
- Internal plugin API remains stable (public interface through `Plugin` trait unchanged)
- Internal function signatures in `dbus.rs` will change to use `nmrs` types

**Systems:**
- No impact on NetworkManager itself (still using D-Bus protocol)
- No impact on UI components (they consume the same store operations)
