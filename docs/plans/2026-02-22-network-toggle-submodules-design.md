# Design: Extract Network Toggle Sub-modules

**Date:** 2026-02-22
**Branch:** larger-larger-picture
**File being refactored:** `crates/overview/src/components/toggles/network.rs`

## Problem

`network.rs` is 1081 lines handling four distinct network feature types (WiFi, Wired/Ethernet, VPN, Tethering) in a single file. The four `update_*` methods are each self-contained but intermingled, making the file hard to navigate and reason about.

## Approach: Sub-modules within `network/mod.rs`

Convert `network.rs` to a directory module (`network/mod.rs`). Each network feature type becomes a private sub-module containing its update logic.

## File Structure (after)

```
toggles/network/
  mod.rs                ← NetworkManagerToggles, ToggleEntry, NetworkRow,
                           adapter_icon(), adapter_title(), build_settings_button(),
                           build_info_row() — shared types and entry point
  wifi.rs               ← update_wifi_menus() + WiFi row building
  wired.rs              ← update_ethernet_menus() + update_wired_info_rows()
  vpn.rs                ← update_vpn_menu_rows() + vpn_icon_name()
  tethering.rs          ← update_tethering_menus()
  network_menu_logic.rs ← unchanged — WiFi pure logic + tests
```

## Symbol Mapping

| Symbol | Destination |
|---|---|
| `update_wifi_menus()` | `wifi.rs` |
| `update_ethernet_menus()` | `wired.rs` |
| `update_wired_info_rows()` | `wired.rs` |
| `update_vpn_menu_rows()` | `vpn.rs` |
| `vpn_icon_name()` | `vpn.rs` |
| `update_tethering_menus()` | `tethering.rs` |
| `ToggleEntry`, `NetworkRow` | stays in `mod.rs` |
| `adapter_icon()`, `adapter_title()` | stays in `mod.rs` |
| `build_settings_button()`, `build_info_row()` | stays in `mod.rs` |
| `NetworkManagerToggles` | stays in `mod.rs` |

## Visibility Rules

- Extracted functions: `pub(super)` — visible to `mod.rs`, not external callers
- `ToggleEntry`, `NetworkRow`: `pub(super)` — accessible from sub-modules
- Helper functions (`build_settings_button`, etc.): `pub(super)` — used by `mod.rs` and potentially sub-modules
- External API (`NetworkManagerToggles`, `as_feature_toggles`): unchanged, stays `pub`

## Behavior

Zero behavior change. Pure file reorganization — move functions, fix visibility modifiers, add `mod` declarations in `mod.rs`. All existing tests in `network_menu_logic.rs` pass unchanged.
