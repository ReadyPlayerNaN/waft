## 1. Sunsetr hangs the application - ✅ FIXED & VERIFIED

**Status:** COMPLETE. All tests passed.

**Fixed:**
- ✅ Runtime mixing bug (tokio in glib) causing busy-polling - moved to tokio::spawn
- ✅ State logic now represents "process running" not "night period"
- ✅ Period-aware labels: "Denní režim do {čas}" / "Noční světlo do {čas}"

**Verified:**
- ✅ No hang when clicking toggle during daylight
- ✅ Toggle shows "on" when sunsetr runs during day
- ✅ Labels display correctly for both periods

**Implementation:** `openspec/changes/fix-sunsetr-and-unify-toggles/`

## 2. Sunsetr label - ✅ FIXED & VERIFIED

The sunsetr feature toggle now displays period-aware labels based on current mode.
- "Denní režim do HH:MM" when in day mode
- "Noční světlo do HH:MM" when in night mode

## 3. Universal Feature toggle component - DEFERRED

**Status:** Design complete, implementation deferred for future work.

**Reasoning:** This is a non-trivial refactor affecting multiple plugins. The design
is documented in the OpenSpec change, but implementation should be done when:
1. Critical bugs are resolved (done ✅)
2. Can be properly tested (needs running app)
3. All plugins can be migrated at once

**Design:** See `openspec/changes/fix-sunsetr-and-unify-toggles/design.md`
- Single component renders both MainButton and ExpandButton
- CSS class "expandable" controls expand button visibility
- No widget rebuilding on state changes

**Decision:** Implemented Task 4 using existing FeatureToggleExpandableWidget instead.

## 4. Sunsetr preset menu - ✅ IMPLEMENTED

**Status:** Complete. Implemented using existing FeatureToggleExpandableWidget.

Implementation:
- ✅ Migrated sunsetr to FeatureToggleExpandableWidget
- ✅ Created PresetMenuWidget to display available presets
- ✅ Added IPC functions: `query_presets()` and `set_preset()`
- ✅ Lazy loading: presets queried when menu expanded
- ✅ Preset switching via `sunsetr preset <name>`
- ✅ Status refresh after preset switch

**Implementation:** Commit `29ad311`

## 5. Plugins to implement

- Tether plugin?
- SNI
- Caffeine (completed separately)

## 6. NetworkManager plugin enhancements

- WiFi: Support connecting to new (unsaved) networks with password prompt
- WiFi: Signal strength icon updates in toggle (currently just on/off)
