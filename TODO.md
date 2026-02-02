## 1. Sunsetr hangs the application - PARTIALLY FIXED

**Status:** Critical busy-polling bug FIXED. State logic FIXED. Needs testing.

**Fixed:**
- ✅ Runtime mixing bug (tokio in glib) causing busy-polling - moved to tokio::spawn
- ✅ State logic now represents "process running" not "night period"
- ✅ Period-aware labels: "Denní režim do {čas}" / "Noční světlo do {čas}"

**Needs Testing:**
- [ ] Verify no hang when clicking toggle during daylight
- [ ] Verify toggle shows "on" when sunsetr runs during day
- [ ] Verify labels display correctly for both periods

**Remaining (lower priority):**
- Preset menu (Task 4) - requires expand button implementation

See: `openspec/changes/fix-sunsetr-and-unify-toggles/` for full spec

## 2. Sunsetr label - FIXED

The sunsetr feature toggle now displays period-aware labels based on current mode.

## 3. Universal Feature toggle component - TODO

**Status:** Design complete, implementation deferred.

**Reasoning:** This is a non-trivial refactor affecting multiple plugins. The design
is documented in the OpenSpec change, but implementation should be done when:
1. Critical bugs are resolved (done)
2. Can be properly tested (needs running app)
3. All plugins can be migrated at once

**Design:** See `openspec/changes/fix-sunsetr-and-unify-toggles/design.md`
- Single component renders both MainButton and ExpandButton
- CSS class "expandable" controls expand button visibility
- No widget rebuilding on state changes

## 4. Sunsetr options - TODO

**Status:** Blocked by Task 3.

Requires expandable toggle to be implemented first. Design includes:
- Query `sunsetr preset list --json` on menu expand
- Populate menu with available presets
- Switch via `sunsetr preset set <name>`

## 5. Plugins to implement

- Tether plugin?
- SNI
- Caffeine (completed separately)

## 6. NetworkManager plugin enhancements

- WiFi: Support connecting to new (unsaved) networks with password prompt
- WiFi: Signal strength icon updates in toggle (currently just on/off)
