# Network Toggle Sub-modules Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Split `crates/overview/src/components/toggles/network.rs` into five focused sub-modules (mod.rs, wifi.rs, wired.rs, vpn.rs, tethering.rs) with zero behavior change.

**Architecture:** Convert the flat `network.rs` to a directory module `network/mod.rs`. Move each `update_*` function to its corresponding feature file. Shared types (`ToggleEntry`, `NetworkRow`) and helpers stay in `mod.rs`. Visibility is adjusted to `pub(super)` throughout.

**Tech Stack:** Rust, GTK4 — no new dependencies.

---

### Task 1: Convert `network.rs` to `network/mod.rs`

The `network/` directory already exists (it holds `network_menu_logic.rs`). Rust allows a module to be defined either as `foo.rs` or `foo/mod.rs`. We need to rename the file.

**Files:**
- Rename: `crates/overview/src/components/toggles/network.rs` → `crates/overview/src/components/toggles/network/mod.rs`

**Step 1: Move the file**

```bash
mv crates/overview/src/components/toggles/network.rs \
   crates/overview/src/components/toggles/network/mod.rs
```

**Step 2: Verify the build still compiles**

```bash
cargo build -p waft-overview 2>&1 | head -30
```

Expected: no errors (the rename is transparent to Rust — `toggles/mod.rs` declares `pub mod network;` which now resolves to `network/mod.rs`).

**Step 3: Commit**

```bash
git add crates/overview/src/components/toggles/network/mod.rs
git add crates/overview/src/components/toggles/network.rs
git commit -m "refactor(overview): convert network.rs to network/mod.rs"
```

---

### Task 2: Extract VPN logic to `vpn.rs`

VPN is the cleanest extraction — `update_vpn_menu_rows()` and `vpn_icon_name()` have no dependency on adapter subscription logic.

**Files:**
- Create: `crates/overview/src/components/toggles/network/vpn.rs`
- Modify: `crates/overview/src/components/toggles/network/mod.rs`

**Step 1: Create `vpn.rs` with the two functions**

The functions reference types from `mod.rs` — use `super::` to access them.

```rust
//! VPN toggle menu rows.

use std::rc::Rc;

use waft_client::EntityActionCallback;
use waft_protocol::Urn;
use waft_protocol::entity;
use waft_ui_gtk::widgets::connection_row::{
    ConnectionRow, ConnectionRowOutput, ConnectionRowProps,
};

use super::{NetworkRow, ToggleEntry};

/// Update VPN menu rows inside the consolidated VPN toggle.
///
/// Uses ConnectionRow widgets with incremental updates instead of
/// full drain+recreate.
pub(super) fn update_vpn_menu_rows(
    entry: &ToggleEntry,
    vpns: &[(Urn, entity::network::Vpn)],
    action_callback: &EntityActionCallback,
) {
    let mut network_rows = entry.network_rows.borrow_mut();

    // Remove rows for VPNs that no longer exist
    let current_vpn_urns: Vec<String> = vpns
        .iter()
        .map(|(urn, _)| urn.as_str().to_string())
        .collect();
    network_rows.retain(|row| {
        if current_vpn_urns.iter().any(|u| u == row.urn_str()) {
            true
        } else {
            row.remove_from(&entry.menu.root());
            false
        }
    });

    // Update existing or create new rows
    for (vpn_urn, vpn) in vpns {
        let vpn_urn_str = vpn_urn.as_str().to_string();
        let active = vpn.state == entity::network::VpnState::Connected;
        let transitioning = matches!(
            vpn.state,
            entity::network::VpnState::Connecting | entity::network::VpnState::Disconnecting
        );

        if let Some(existing) = network_rows.iter().find(|r| r.urn_str() == vpn_urn_str) {
            // Update existing ConnectionRow
            if let NetworkRow::Connection { row, .. } = existing {
                row.set_name(&vpn.name);
                row.set_active(active);
                row.set_transitioning(transitioning);
            }
        } else {
            // Create new ConnectionRow
            let conn_row = Rc::new(ConnectionRow::new(ConnectionRowProps {
                name: vpn.name.clone(),
                active,
                transitioning,
                icon: Some(vpn_icon_name(&vpn.vpn_type)),
            }));

            let action_cb = action_callback.clone();
            let urn_for_click = vpn_urn.clone();
            let vpn_state = vpn.state;
            conn_row.connect_output(move |ConnectionRowOutput::Toggle| {
                let action = match vpn_state {
                    entity::network::VpnState::Connected => "disconnect",
                    entity::network::VpnState::Disconnected => "connect",
                    // Don't send actions during transitions
                    _ => return,
                };
                action_cb(
                    urn_for_click.clone(),
                    action.to_string(),
                    serde_json::Value::Null,
                );
            });

            entry.menu.append(&conn_row.root);

            network_rows.push(NetworkRow::Connection {
                urn_str: vpn_urn_str,
                row: conn_row,
            });
        }
    }
}

/// Determine the icon name for a VPN connection based on its type.
pub(super) fn vpn_icon_name(vpn_type: &entity::network::VpnType) -> String {
    match vpn_type {
        entity::network::VpnType::Wireguard => "network-vpn-symbolic".to_string(),
        entity::network::VpnType::Vpn => "network-vpn-symbolic".to_string(),
    }
}
```

**Step 2: Add `mod vpn;` to `mod.rs` and remove the two functions from it**

At the top of `network/mod.rs`, add:
```rust
mod vpn;
```

Then in `mod.rs`, delete:
- `fn update_vpn_menu_rows(...)` (full function body)
- `fn vpn_icon_name(...)` (full function body)

Update the call sites in `mod.rs` to use the module path. Search for `Self::update_vpn_menu_rows` and replace with `vpn::update_vpn_menu_rows`:

```rust
// Before:
Self::update_vpn_menu_rows(&entry, &vpns, &cb);
// ...
Self::update_vpn_menu_rows(entry, &vpns, &cb);

// After:
vpn::update_vpn_menu_rows(&entry, &vpns, &cb);
// ...
vpn::update_vpn_menu_rows(entry, &vpns, &cb);
```

**Step 3: Make shared types `pub(super)` in `mod.rs`**

`ToggleEntry` and `NetworkRow` are currently private. Change them to `pub(super)`:

```rust
pub(super) struct ToggleEntry { ... }
pub(super) enum NetworkRow { ... }
```

**Step 4: Build and verify**

```bash
cargo build -p waft-overview 2>&1 | head -40
```

Expected: clean build.

**Step 5: Run tests**

```bash
cargo test -p waft-overview 2>&1
```

Expected: all tests pass.

**Step 6: Commit**

```bash
git add crates/overview/src/components/toggles/network/vpn.rs \
        crates/overview/src/components/toggles/network/mod.rs
git commit -m "refactor(overview): extract VPN menu logic to network/vpn.rs"
```

---

### Task 3: Extract WiFi logic to `wifi.rs`

**Files:**
- Create: `crates/overview/src/components/toggles/network/wifi.rs`
- Modify: `crates/overview/src/components/toggles/network/mod.rs`

**Step 1: Create `wifi.rs`**

Copy `update_wifi_menus()` from `mod.rs` into `wifi.rs`. All imports come from `mod.rs`'s existing import list — bring the ones needed here.

```rust
//! WiFi network menu rows for wireless adapters.

use std::cell::Cell;
use std::rc::Rc;

use gtk::prelude::*;
use waft_client::{EntityActionCallback, EntityStore};
use waft_protocol::Urn;
use waft_protocol::entity;
use waft_ui_gtk::icons::IconWidget;

use super::{NetworkRow, ToggleEntry, should_be_expandable, details_text};

pub(super) fn update_wifi_menus(
    entries: &Rc<std::cell::RefCell<Vec<ToggleEntry>>>,
    store: &Rc<EntityStore>,
    action_callback: &EntityActionCallback,
    settings_available: &Rc<Cell<bool>>,
) {
    // ... exact body of update_wifi_menus from mod.rs ...
}
```

Note: `should_be_expandable` and `details_text` come from `network_menu_logic` which is a sibling module. In `mod.rs` they are re-exported via `use network_menu_logic::{details_text, should_be_expandable}`. In `wifi.rs` reference them via `super::details_text` and `super::should_be_expandable` (since `mod.rs` re-exports them into the `super` namespace).

**Step 2: Add `mod wifi;` to `mod.rs` and remove `update_wifi_menus` from it**

```rust
mod wifi;
```

Replace `Self::update_wifi_menus(...)` call sites with `wifi::update_wifi_menus(...)`.

**Step 3: Build and test**

```bash
cargo build -p waft-overview && cargo test -p waft-overview
```

**Step 4: Commit**

```bash
git add crates/overview/src/components/toggles/network/wifi.rs \
        crates/overview/src/components/toggles/network/mod.rs
git commit -m "refactor(overview): extract WiFi menu logic to network/wifi.rs"
```

---

### Task 4: Extract Wired/Ethernet logic to `wired.rs`

**Files:**
- Create: `crates/overview/src/components/toggles/network/wired.rs`
- Modify: `crates/overview/src/components/toggles/network/mod.rs`

**Step 1: Create `wired.rs`**

Move both `update_ethernet_menus()` and `update_wired_info_rows()` into `wired.rs`. These two functions call each other, so they belong together.

```rust
//! Wired/Ethernet connection profile and IP info rows.

use std::cell::Cell;
use std::rc::Rc;

use gtk::prelude::*;
use waft_client::{EntityActionCallback, EntityStore};
use waft_protocol::Urn;
use waft_protocol::entity;
use waft_ui_gtk::icons::IconWidget;

use super::{NetworkRow, ToggleEntry, build_info_row};

pub(super) fn update_ethernet_menus(
    entries: &Rc<std::cell::RefCell<Vec<ToggleEntry>>>,
    store: &Rc<EntityStore>,
    action_callback: &EntityActionCallback,
    settings_available: &Rc<Cell<bool>>,
) {
    // ... exact body ...
}

pub(super) fn update_wired_info_rows(
    entry: &ToggleEntry,
    adapter: &entity::network::NetworkAdapter,
    settings_available: &Rc<Cell<bool>>,
) {
    // ... exact body ...
}
```

Note: `update_wired_info_rows` calls `build_info_row` which stays in `mod.rs`. Access it as `super::build_info_row(...)`.

**Step 2: Add `mod wired;` to `mod.rs` and remove both functions**

Replace call sites:
- `Self::update_ethernet_menus(...)` → `wired::update_ethernet_menus(...)`
- `update_wired_info_rows(...)` (free function calls inside `mod.rs`) → `wired::update_wired_info_rows(...)`

Make `build_info_row` `pub(super)` in `mod.rs` since `wired.rs` now calls it.

**Step 3: Build and test**

```bash
cargo build -p waft-overview && cargo test -p waft-overview
```

**Step 4: Commit**

```bash
git add crates/overview/src/components/toggles/network/wired.rs \
        crates/overview/src/components/toggles/network/mod.rs
git commit -m "refactor(overview): extract wired/ethernet menu logic to network/wired.rs"
```

---

### Task 5: Extract Tethering logic to `tethering.rs`

**Files:**
- Create: `crates/overview/src/components/toggles/network/tethering.rs`
- Modify: `crates/overview/src/components/toggles/network/mod.rs`

**Step 1: Create `tethering.rs`**

```rust
//! Tethering connection rows for hotspot client adapters.

use std::rc::Rc;

use waft_client::{EntityActionCallback, EntityStore};
use waft_protocol::Urn;
use waft_protocol::entity;
use waft_ui_gtk::widgets::connection_row::{
    ConnectionRow, ConnectionRowOutput, ConnectionRowProps,
};

use super::{NetworkRow, ToggleEntry};

pub(super) fn update_tethering_menus(
    entries: &Rc<std::cell::RefCell<Vec<ToggleEntry>>>,
    store: &Rc<EntityStore>,
    action_callback: &EntityActionCallback,
) {
    // ... exact body of update_tethering_menus from mod.rs ...
}
```

**Step 2: Add `mod tethering;` to `mod.rs` and remove `update_tethering_menus`**

Replace call site: `Self::update_tethering_menus(...)` → `tethering::update_tethering_menus(...)`.

**Step 3: Final build and full test**

```bash
cargo build --workspace && cargo test --workspace
```

Expected: clean build, all tests pass.

**Step 4: Final commit**

```bash
git add crates/overview/src/components/toggles/network/tethering.rs \
        crates/overview/src/components/toggles/network/mod.rs
git commit -m "refactor(overview): extract tethering menu logic to network/tethering.rs"
```

---

### Final State Verification

After all tasks, the directory should look like:

```
crates/overview/src/components/toggles/network/
  mod.rs                ← ~550 lines (NetworkManagerToggles, ToggleEntry, NetworkRow, helpers)
  wifi.rs               ← ~136 lines
  wired.rs              ← ~175 lines
  vpn.rs                ← ~100 lines
  tethering.rs          ← ~86 lines
  network_menu_logic.rs ← unchanged
```

Verify with:
```bash
wc -l crates/overview/src/components/toggles/network/*.rs
```
