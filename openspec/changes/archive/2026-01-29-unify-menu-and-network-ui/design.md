## Context

The codebase currently has three different UI patterns for menu items:
1. **Slider controls** - Use a clean row-based layout with icon, label, and right-side control
2. **Bluetooth device menu** - Custom DeviceRow implementation with icon, name label, spinner, and switch
3. **WiFi menu** - Similar custom implementation with network items

The networkmanager wired plugin uses a custom EthernetToggleWidget that doesn't follow the expandable toggle pattern used by WiFi, has a hardcoded "Disabled" string (line 40 in ethernet_toggle.rs), doesn't display connection details, and has issues with connection status display.

Current architecture uses GTK4 with Rust, employing the plugin pattern where each feature (bluetooth, networkmanager, audio) implements the Plugin trait. UI components are in `src/ui/` and feature-specific widgets are in `src/features/<plugin>/`.

## Goals / Non-Goals

**Goals:**
- Extract a reusable MenuItemWidget component from slider control pattern
- Make menu items clickable with primary action support (clicking row = toggle action)
- Standardize bluetooth and wifi menus to use the new component
- Refactor ethernet toggle to use FeatureToggleExpandableWidget pattern
- Display connection details (IP address, subnet mask, gateway) when ethernet is expanded
- Fix connection status display logic in networkmanager wired
- Replace hardcoded "Disabled" with i18n translation

**Non-Goals:**
- Changing the visual design or styling
- Modifying VPN or other networkmanager components
- Adding new network configuration capabilities
- Refactoring audio or other plugin menus (out of scope for this change)

## Decisions

### Decision 1: Extract MenuItemWidget as simple clickable container

**Rationale:** Menu items need consistent styling and click behavior across plugins. A simple, dumb component that provides design and click handling allows maximum flexibility.

**Alternatives considered:**
- **Property-based approach (icon, text, etc.):** Too rigid, doesn't handle all use cases, more maintenance
- **Shared trait approach:** Requires more boilerplate without actual widget reuse
- **Keep duplicated code:** Maintains current inconsistency and makes future changes harder

**Choice:** Extract as `src/ui/menu_item.rs` - a simple clickable styled container:
- **API:** `MenuItemWidget::new(child: impl IsA<gtk::Widget>, on_click: impl Fn())`
- **Behavior:** Always clickable, provides consistent menu item styling
- **Responsibility:** Only handles click events and applies CSS styling
- **Flexibility:** Consuming code builds the content structure (icon + labels + switch/etc.)

**Example usage:**
```rust
// Consuming code builds the content
let content = gtk::Box::new(Horizontal, 12);
content.append(&icon);
content.append(&label);
content.append(&switch);

// MenuItemWidget just wraps it with style + click
let menu_item = MenuItemWidget::new(content, || {
    // Handle click action
});
```

### Decision 2: MenuItemWidget is always clickable

**Rationale:** Better UX - users can click anywhere on the row to invoke the action. Making clickability mandatory simplifies the API and ensures consistent behavior.

**Implementation:**
- The entire menu item is a clickable button-like container
- Applies hover/active states for visual feedback
- Click handler is mandatory - passed during construction
- CSS class `menu-item` for styling

**Trade-off:** If a menu item shouldn't be clickable, don't use MenuItemWidget (or pass no-op handler). This is acceptable since non-clickable menu items are rare in this UI.

### Decision 3: Migrate ethernet to FeatureToggleExpandableWidget (one per adapter)

**Rationale:** WiFi already uses this pattern successfully - one FeatureToggleExpandableWidget per adapter. Wired interfaces should follow the same pattern for consistency.

**Current:** Single custom EthernetToggleWidget with no expand capability
**New:** One FeatureToggleExpandableWidget per wired adapter (same as WiFi):
- Title: "Wired ({interface_name})"
- Icon: network-wired-* (varies by status)
- Details: Connection status or "Disabled"
- Expandable menu showing connection information

**Multiple adapters:** Just like WiFi creates one toggle per wireless adapter, create one toggle per wired adapter. Most systems have one, but USB-Ethernet or docks can add more.

**Migration approach:**
- Replace EthernetToggleWidget with FeatureToggleExpandableWidget in mod.rs
- Create one toggle per wired device (iterate over devices like WiFi does)
- Move connection display logic to new EthernetMenuWidget (like WiFiMenuWidget)
- Use existing expand callback pattern from WiFi implementation

### Decision 4: Connection details display structure

**Information to show when expanded:**
- Link speed (e.g., "1 Gbps", "100 Mbps")
- IP address (IPv4 and IPv6 if available)
- Subnet mask
- Default gateway

**Data source:** NetworkManager DBus API:
- Get active connection via `ActiveConnection` property
- Query connection settings via `org.freedesktop.NetworkManager.IP4Config`
- Query connection settings via `org.freedesktop.NetworkManager.IP6Config`
- Get link speed from device properties

**Layout:** Vertical list of label-value pairs, similar to system settings

### Decision 5: i18n for "Disabled" label

**Current issue:** Hardcoded "Disabled" string in ethernet_toggle.rs line 40

**Fix:**
- Add translation key: `"network-disabled"` → translations.json
- Use `crate::i18n::t("network-disabled")` instead of literal string
- Apply same pattern to other status labels ("Connected", "Disconnected", etc.)

## Risks / Trade-offs

**[Risk]** Extracting MenuItemWidget might not fit all future use cases
**→ Mitigation:** Keep the component generic with optional properties. Can extend without breaking existing usage.

**[Risk]** Making rows clickable could interfere with right-side widget interaction
**→ Mitigation:** Properly handle event propagation. When right widget is clicked, stop propagation to prevent double-triggering.

**[Risk]** NetworkManager DBus queries for IP info could be slow
**→ Mitigation:** Cache connection details, only refresh on expand or connection change events.

**[Risk]** Breaking existing bluetooth/wifi functionality during migration
**→ Mitigation:** Incremental migration (one plugin at a time), thorough testing of toggle/connect/disconnect flows.

**[Trade-off]** Adding connection details increases UI complexity
**→ Accepted:** Users expect this information. WiFi shows similar details, ethernet should too.

## Migration Plan

**Phase 1: Extract component**
1. Create `src/ui/menu_item.rs` with MenuItemWidget
2. Add tests for the new component
3. No breaking changes yet

**Phase 2: Migrate bluetooth**
1. Update DeviceRow to use MenuItemWidget
2. Implement click handler for connect/disconnect action
3. Test all bluetooth device operations

**Phase 3: Migrate wifi** (if needed)
1. Similar to bluetooth migration
2. Verify network selection and connection works

**Phase 4: Refactor ethernet**
1. Replace EthernetToggleWidget with FeatureToggleExpandableWidget
2. Create EthernetMenuWidget for connection details
3. Implement DBus queries for IP/gateway/mask
4. Add i18n for status labels
5. Test wired connection detection and display

**Rollback:** Each phase is independent. If issues arise, revert the specific component change without affecting others.
