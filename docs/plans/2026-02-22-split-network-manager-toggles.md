# Split NetworkManagerToggles

## Why

`NetworkManagerToggles` is a 528-line monolith that handles four conceptually independent toggle types (WiFi, Wired, VPN, Tethering) in a single struct with a shared `entries: Vec<ToggleEntry>`. This causes:
- Tight coupling: WiFi subscription code must know about VPN entries to avoid conflicts
- Fragile URN-based filtering (`entry.urn_str.contains("/network-adapter/")`) instead of type-safe separation
- A single `rebuild_callback` that fires for all network changes regardless of type
- Hard to test or extend one toggle type without understanding all four

Splitting into four independent toggle structs (`WifiToggles`, `WiredToggles`, `VpnToggles`, `TetheringToggles`) makes each self-contained with its own entity subscriptions and its own `entries: Vec<ToggleEntry>`.

## What Changes

1. **Extract** `WifiToggles` -- owns wireless adapter entries, subscribes to `network-adapter` (filtered to `AdapterKind::Wireless`) and `wifi-network`
2. **Extract** `WiredToggles` -- owns wired adapter entries, subscribes to `network-adapter` (filtered to `AdapterKind::Wired`) and `ethernet-connection`
3. **Extract** `VpnToggles` -- owns the consolidated VPN toggle entry, subscribes to `vpn`
4. **Extract** `TetheringToggles` -- owns tethering adapter entries, subscribes to `network-adapter` (filtered to `AdapterKind::Tethering`) and `tethering-connection`
5. **Update** `default.xml` to replace `<NetworkToggles />` with four separate elements
6. **Keep** shared types (`ToggleEntry`, `NetworkRow`, helper functions) in `network/mod.rs`

## Affected Files

- `crates/overview/src/components/toggles/network/mod.rs` -- keep `ToggleEntry`, `NetworkRow`, `adapter_icon`, `adapter_title`; remove `NetworkManagerToggles`
- `crates/overview/src/components/toggles/network/wifi.rs` -- expand to `WifiToggles` struct with `new()` and `as_feature_toggles()`
- `crates/overview/src/components/toggles/network/wired.rs` -- expand to `WiredToggles` struct with `new()` and `as_feature_toggles()`
- `crates/overview/src/components/toggles/network/vpn.rs` -- expand to `VpnToggles` struct with `new()` and `as_feature_toggles()`
- `crates/overview/src/components/toggles/network/tethering.rs` -- expand to `TetheringToggles` struct with `new()` and `as_feature_toggles()`
- `crates/overview/src/components/toggles/network/network_menu_logic.rs` -- unchanged (pure functions)
- `crates/overview/src/layout/default.xml` -- replace `<NetworkToggles />` with `<WifiToggles />`, `<WiredToggles />`, `<VpnToggles />`, `<TetheringToggles />`
- `crates/overview/src/layout/renderer.rs` -- replace `NetworkManagerToggles` match arm with four separate arms; add four `DynamicToggleSource` impls

## Tasks

### 1. Add shared re-exports to `crates/overview/src/components/toggles/network/mod.rs`

Keep `ToggleEntry`, `NetworkRow`, `adapter_icon`, `adapter_title` as `pub(super)` or `pub(crate)`. Remove `NetworkManagerToggles` struct and its `impl` block. Add public re-exports for the four new toggle types:

```rust
pub use wifi::WifiToggles;
pub use wired::WiredToggles;
pub use vpn::VpnToggles;
pub use tethering::TetheringToggles;
```

### 2. Create `WifiToggles` in `crates/overview/src/components/toggles/network/wifi.rs`

Extract the WiFi-specific logic from `NetworkManagerToggles::new()`:

```rust
pub struct WifiToggles {
    entries: Rc<RefCell<Vec<ToggleEntry>>>,
    store: Rc<EntityStore>,
    action_callback: EntityActionCallback,
    menu_store: Rc<MenuStore>,
    settings_tracker: Rc<SettingsAppTracker>,
}
```

- `new()` subscribes to `network-adapter` (filter `AdapterKind::Wireless` only) and `wifi-network`
- Move the wireless adapter creation/update code from the adapter subscription closure
- Move `update_wifi_menus` call integration
- `as_feature_toggles()` returns only WiFi adapter toggles

### 3. Create `WiredToggles` in `crates/overview/src/components/toggles/network/wired.rs`

Extract the Wired-specific logic:

```rust
pub struct WiredToggles {
    entries: Rc<RefCell<Vec<ToggleEntry>>>,
    store: Rc<EntityStore>,
    action_callback: EntityActionCallback,
    menu_store: Rc<MenuStore>,
    settings_tracker: Rc<SettingsAppTracker>,
}
```

- `new()` subscribes to `network-adapter` (filter `AdapterKind::Wired` only) and `ethernet-connection`
- Move wired adapter creation/update code and `update_wired_info_rows` calls
- Move `update_ethernet_menus` call integration
- `as_feature_toggles()` returns only wired adapter toggles

### 4. Create `VpnToggles` in `crates/overview/src/components/toggles/network/vpn.rs`

Extract the VPN-specific logic:

```rust
pub struct VpnToggles {
    entries: Rc<RefCell<Vec<ToggleEntry>>>,
    store: Rc<EntityStore>,
    action_callback: EntityActionCallback,
    menu_store: Rc<MenuStore>,
}
```

- `new()` subscribes to `vpn` entity type
- Move consolidated VPN toggle creation/update code
- Move `update_vpn_menu_rows` call integration
- `as_feature_toggles()` returns the VPN toggle (0 or 1 entries)

### 5. Create `TetheringToggles` in `crates/overview/src/components/toggles/network/tethering.rs`

Extract the Tethering-specific logic:

```rust
pub struct TetheringToggles {
    entries: Rc<RefCell<Vec<ToggleEntry>>>,
    store: Rc<EntityStore>,
    action_callback: EntityActionCallback,
    menu_store: Rc<MenuStore>,
}
```

- `new()` subscribes to `network-adapter` (filter `AdapterKind::Tethering` only) and `tethering-connection`
- Move tethering adapter creation/update code and `update_tethering_menus` call
- `as_feature_toggles()` returns only tethering adapter toggles

### 6. Remove `NetworkManagerToggles` from `crates/overview/src/components/toggles/network/mod.rs`

Delete the `NetworkManagerToggles` struct, its `impl` block, and the `SettingsAppTracker` field handling that was shared across types. Each toggle type now owns its own `SettingsAppTracker` if it needs one (WiFi and Wired need it; VPN and Tethering do not).

### 7. Update `crates/overview/src/layout/default.xml`

Replace:
```xml
<NetworkToggles />
```
with:
```xml
<WifiToggles />
<WiredToggles />
<VpnToggles />
<TetheringToggles />
```

### 8. Update `crates/overview/src/layout/renderer.rs`

Replace the `"NetworkToggles"` match arm:

```rust
"NetworkToggles" => {
    let net = Rc::new(NetworkManagerToggles::new(...));
    dynamic_sources.push(net.clone());
    keep.push(Box::new(net));
}
```

with four separate arms:

```rust
"WifiToggles" => {
    let t = Rc::new(WifiToggles::new(&ctx.store, &ctx.action_callback, menu_store, dynamic_rebuild.clone()));
    dynamic_sources.push(t.clone());
    keep.push(Box::new(t));
}
"WiredToggles" => {
    let t = Rc::new(WiredToggles::new(&ctx.store, &ctx.action_callback, menu_store, dynamic_rebuild.clone()));
    dynamic_sources.push(t.clone());
    keep.push(Box::new(t));
}
"VpnToggles" => {
    let t = Rc::new(VpnToggles::new(&ctx.store, &ctx.action_callback, menu_store, dynamic_rebuild.clone()));
    dynamic_sources.push(t.clone());
    keep.push(Box::new(t));
}
"TetheringToggles" => {
    let t = Rc::new(TetheringToggles::new(&ctx.store, &ctx.action_callback, menu_store, dynamic_rebuild.clone()));
    dynamic_sources.push(t.clone());
    keep.push(Box::new(t));
}
```

Add four `DynamicToggleSource` impls (replacing the single `NetworkManagerToggles` impl).

Update the imports to use the four new types instead of `NetworkManagerToggles`.

### 9. Remove `SettingsAppTracker` sharing from adapter subscription

Each toggle type that needs settings button support (`WifiToggles`, `WiredToggles`) creates its own `SettingsAppTracker`. The `settings_available: Rc<Cell<bool>>` is local to each toggle type. `VpnToggles` and `TetheringToggles` do not need settings buttons and skip `SettingsAppTracker` entirely.

### 10. Run `cargo build --workspace` and `cargo test --workspace`

Verify compilation succeeds and `network_menu_logic` tests pass.
