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

## 3. Universal Feature toggle component - ✅ COMPLETE

**Status:** Implemented and verified.

Implementation:
- ✅ Redesigned FeatureToggleWidget with Box root (was Button)
- ✅ Always renders both MainButton and ExpandButton
- ✅ CSS class "expandable" controls expand button visibility
- ✅ Added `set_expandable()` method for runtime switching
- ✅ Integrated MenuStore and expand callback support
- ✅ Added CSS rule: `.feature-toggle:not(.expandable) .toggle-expand { display: none; }`
- ✅ Migrated caffeine, darkman, and notifications plugins
- ✅ Migrated sunsetr to use unified component with dynamic expandability

**Design:** See `openspec/changes/fix-sunsetr-and-unify-toggles/design.md`

## 4. Sunsetr preset menu - ✅ COMPLETE & VERIFIED

**Status:** Complete. Fully functional with dynamic expandability.

Implementation:
- ✅ Migrated sunsetr to unified FeatureToggleWidget
- ✅ Created PresetMenuWidget to display available presets
- ✅ Added IPC functions: `query_presets()` and `set_preset()`
- ✅ Lazy loading: presets queried when menu expanded
- ✅ Preset switching via `sunsetr preset <name>`
- ✅ Status refresh after preset switch
- ✅ **Dynamic expandability**: expand button only shows when sunsetr is active

**Key feature:** Toggle is simple when sunsetr is OFF, expandable when ON.

## 5. Plugins to implement

- Tether plugin?
- SNI
- Caffeine (completed separately)

## 6. NetworkManager plugin enhancements

- WiFi: Support connecting to new (unsaved) networks with password prompt
- WiFi: Signal strength icon updates in toggle (currently just on/off)
