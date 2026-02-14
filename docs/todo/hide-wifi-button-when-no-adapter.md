# Hide WiFi Settings Button When No Adapter Present

**Status:** Implemented

## Goal

Hide the WiFi category button in the `waft-settings` sidebar when no WiFi adapter is present. Show it again when an adapter appears.

## Current State

- **Sidebar** (`crates/settings/src/sidebar.rs`): Dumb widget with hardcoded rows (Bluetooth, WiFi, Wired, Display). All rows are always visible.
- **WiFiPage** (`crates/settings/src/pages/wifi.rs`): Smart container that already filters for `AdapterKind::Wireless` when reconciling adapters.
- **App** (`crates/settings/src/app.rs`): Subscribes to `ADAPTER_ENTITY_TYPE` and creates `EntityStore`.

## Problem

The sidebar currently creates all category rows at construction time with no mechanism to show/hide them dynamically based on entity state.

## Implementation Plan

### 1. Add Dynamic Visibility to Sidebar (Dumb Widget)

**File:** `crates/settings/src/sidebar.rs`

- **Store WiFi row reference**: Add `wifi_row: adw::ActionRow` field to `Sidebar` struct
- **Add visibility method**: `pub fn set_wifi_visible(&self, visible: bool)`
  - Call `self.wifi_row.set_visible(visible)`
  - If hiding WiFi and it's currently selected, auto-select Bluetooth row (index 0)
- **Initialize hidden**: Start with `wifi_row.set_visible(false)` in constructor
- **Update row selection logic**: `connect_row_selected` should skip invisible rows when mapping index to category

**Rationale:** Sidebar remains a dumb presentational widget. It doesn't know *why* WiFi should be hidden, only *that* it should be hidden when told.

### 2. Detect WiFi Adapter Presence in Window (Smart Container)

**File:** `crates/settings/src/window.rs`

- **Subscribe to adapter changes**: Add entity store subscription for `ADAPTER_ENTITY_TYPE`
  - Filter adapters by `AdapterKind::Wireless`
  - Count wireless adapters
  - Call `sidebar.set_wifi_visible(count > 0)`
- **Store sidebar reference**: Change `sidebar: Sidebar` field to allow mutation, or use `Rc<Sidebar>` if needed for the subscription closure

**Rationale:** The window is the smart container that orchestrates sidebar state based on entity store changes.

### 3. Handle Initial State

**File:** `crates/settings/src/window.rs`

- When entity store initially loads adapters, the subscription will fire
- If no WiFi adapters exist, WiFi row remains hidden
- If WiFi adapters exist, WiFi row becomes visible
- This handles both "never had adapter" and "adapter removed" cases

### 4. Update Row Selection Logic

**File:** `crates/settings/src/sidebar.rs`

Current index mapping in `connect_row_selected`:
```rust
let category = match index {
    0 => "Bluetooth",
    1 => "WiFi",
    2 => "Wired",
    3 => "Display",
    _ => return,
};
```

This breaks if WiFi row is hidden (Wired becomes index 1, not 2).

**Solution Options:**

**Option A (Recommended):** Use `ActionRow.title()` instead of index
```rust
if let Some(title) = row.downcast_ref::<adw::ActionRow>()
    .and_then(|r| r.title().as_str())
{
    if let Some(ref callback) = *cb.borrow() {
        callback(SidebarOutput::Selected(title.to_string()));
    }
}
```

**Option B:** Store row-to-category mapping in a `HashMap<gtk::ListBoxRow, String>` at construction time

**Rationale:** Option A is simpler and more maintainable. Row titles already match category names exactly.

### 5. Handle Page Switching When WiFi Hidden

**File:** `crates/settings/src/window.rs`

- If WiFi page is currently active when WiFi row is hidden, switch to Bluetooth page
- Check `stack.visible_child_name()` before hiding WiFi
- If current page is "WiFi", call `stack.set_visible_child_name("Bluetooth")` and select Bluetooth row

## Testing

1. **Start with no WiFi adapter:**
   - Launch `waft-settings`
   - Verify WiFi button is hidden
   - Verify Bluetooth is selected by default
   - Verify clicking Wired/Display works

2. **Start with WiFi adapter:**
   - Launch `waft-settings`
   - Verify WiFi button is visible
   - Click WiFi, verify page switches correctly

3. **Hot-plug adapter removal:**
   - With WiFi page open, disable/remove WiFi adapter via NetworkManager
   - Verify WiFi button disappears
   - Verify page auto-switches to Bluetooth

4. **Hot-plug adapter addition:**
   - With no WiFi adapter, enable/add one via NetworkManager
   - Verify WiFi button appears
   - Verify clicking it switches to WiFi page

## Open Questions

1. **Should Wired button also be hidden when no Ethernet adapters present?**
   - Likely yes for consistency, but not in this task scope
   - Could follow same pattern: `set_wired_visible(bool)`

2. **Should the sidebar auto-select first visible row if current row becomes hidden?**
   - Yes (covered in step 1 and 5)

## Files Modified

- `crates/settings/src/sidebar.rs` - Added `set_wifi_visible()`, WiFi row starts hidden, title-based selection
- `crates/settings/src/window.rs` - Wired up `ADAPTER_ENTITY_TYPE` subscription with `AdapterKind::Wireless` check, initial visibility via `idle_add_local_once`

## Estimated Complexity

**Low** - Clear pattern, well-defined scope, no new architecture needed.

## Follow-up Work

- Apply same pattern to Wired button (separate task)
- Consider generalizing to `set_category_visible(category: &str, visible: bool)` if more categories need dynamic visibility
