## 1. D-Bus IP Configuration Functions

- [x] 1.1 Add `IpConfiguration` struct to `dbus.rs` with fields: ipv4_address, ipv6_address, subnet_mask, gateway (all `Option<String>`)
- [x] 1.2 Add `prefix_to_subnet_mask(prefix: u32) -> String` helper function to convert CIDR prefix to dotted decimal
- [x] 1.3 Add `get_ip4_config(dbus: &DbusHandle, device_path: &str) -> Result<Option<(String, u32, Option<String>)>>` to fetch IPv4 address, prefix, and gateway from IP4Config object
- [x] 1.4 Add `get_ip6_config(dbus: &DbusHandle, device_path: &str) -> Result<Option<String>>` to fetch IPv6 address from IP6Config object
- [x] 1.5 Add `get_ip_configuration(dbus: &DbusHandle, device_path: &str) -> Result<IpConfiguration>` that combines IP4 and IP6 config into the struct

## 2. Widget Integration

- [x] 2.1 Update `setup_expand_callback` in `wired_adapter_widget.rs` to call `get_ip_configuration` alongside `get_link_speed`
- [x] 2.2 Map `IpConfiguration` fields to `ConnectionDetails` fields in the async callback
- [x] 2.3 Verify the menu displays all connection details when expanded on a connected adapter

## 3. Testing

- [x] 3.1 Test with connected wired adapter - verify IPv4, subnet mask, gateway display correctly
- [x] 3.2 Test with disconnected adapter - verify "Odpojeno" message still appears
- [x] 3.3 Test with IPv6-enabled connection - verify IPv6 address displays
