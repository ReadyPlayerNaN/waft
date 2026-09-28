# Plan: Convert `waft-settings` UI structure to GTK XML templates

## Status

Implementation in progress.

## Progress

- [x] Phase 0 baseline inspection completed.
- [ ] Phase 1 template/resource foundation.
- [ ] Phase 2 application shell.
- [ ] Phase 3 reusable rows and controls.
- [ ] Phase 4 keyed GTK child management and VDOM removal.
- [ ] Phase 5 static and mostly-static sections.
- [ ] Phase 6 dialogs and sub-pages.
- [ ] Phase 7 page composers.
- [ ] Phase 8 CSS/resource ownership.
- [ ] Phase 9 obsolete construction-path cleanup.
- [ ] Phase 10 verification and regression coverage.

## Goal

Move the stable widget hierarchy of `waft-settings` from imperative Rust builders into GTK XML templates, while keeping application state, entity subscriptions, actions, navigation, configuration editing, and dynamic list reconciliation in Rust.

The target architecture is:

```text
GTK XML / CompositeTemplate
    static widget hierarchy, styles, accessibility roles

Rust component
    state, translated values, signal handlers, entity updates, actions

Rust page/controller
    subscriptions, keyed dynamic children, navigation, daemon integration
```

This is a UI-structure migration. It must not change DBus ownership, daemon protocol behavior, plugin behavior, threading boundaries, or settings APIs.

## Scope

Convert the entire `waft-settings` application incrementally, including:

- application window and navigation shell
- sidebar and search presentation
- page shells and static sections
- reusable rows and controls
- dialogs and sub-pages
- entity-driven pages
- direct KDL/configuration pages
- inline CSS and UI resources where appropriate

Dynamic entity data and behavior remain in Rust.

## Non-goals

This plan does not:

- replace `EntityStore`, `WaftClient`, or action callbacks
- move daemon or DBus logic into XML
- encode entity-dependent lists or business rules in XML
- introduce a new UI state-management framework
- require every GTK object to be instantiated from XML
- perform a flag-day rewrite of all pages
- preserve the current VDOM abstraction if explicit GTK widgets provide a simpler result

## Architectural decision

Use XML for **stable widget structure** and Rust for **runtime behavior**.

### XML owns

- widget hierarchy
- static widget properties
- spacing, margins, alignment, and expansion defaults
- stable CSS classes
- template child IDs
- static accessibility metadata where possible
- placeholders for dynamic content

### Rust owns

- `EntityStore` subscriptions
- action dispatch and action result handling
- translated runtime text via `crate::i18n::t`
- dynamic visibility and sensitivity
- model population and selection state
- keyed child creation/update/removal/reordering
- signal callbacks
- navigation and dialog presentation
- KDL/configuration file I/O
- runtime-dependent icons and values

No user-facing strings should be duplicated in XML independently of the existing localization system. XML templates should either use neutral placeholders or receive translated text from Rust.

## Recommended implementation technology

Use GTK/libadwaita composite templates (`CompositeTemplate`) for reusable custom widgets and page components, backed by a GResource-based template bundle.

Avoid scattering ad-hoc `gtk::Builder::from_string()` calls throughout the crate. A single resource-loading convention gives earlier validation, consistent ownership, and predictable packaging.

Before implementation:

- [ ] Confirm the exact `gtk4`/`libadwaita` composite-template API available in the workspace versions.
- [ ] Choose the resource location, recommended: `crates/settings/ui/`.
- [ ] Add one resource manifest and one resource-registration path.
- [ ] Document template naming, IDs, and component ownership conventions.
- [ ] Verify debug and release builds include the same resources.

## Component conventions

Each XML-backed component should follow this shape:

```text
ui/<component>.ui
src/<component>.rs
```

The Rust type should:

- own the template root and relevant child widgets
- expose a focused constructor
- expose `update(&Props)` for state updates where needed
- expose output callbacks for user actions
- disconnect or guard handlers when updates can cause feedback loops
- keep GTK objects on the GTK main thread

For dynamic rows, prefer a concrete GTK widget with stable identity over rebuilding the widget tree.

## Keyed GTK child management

The current WiFi groups already use `HashMap<String, ...>` keyed by URN. This pattern should become explicit and reusable where multiple settings pages need it.

Introduce a named keyed-child abstraction, either in `waft-settings` or `waft-ui-gtk` after inventory confirms reuse. It should support:

- stable key ownership
- create-on-missing
- update-in-place
- remove-stale
- deterministic ordering
- widget removal from the parent
- cleanup of associated callbacks/state
- no duplicate rows after repeated entity updates

Do not use a generic `utils` or `helpers` module for this. A name such as `keyed_widget_list` or `entity_widget_list` is preferable.

The abstraction must work with the containers actually used by the settings app, including `gtk::Box` and `adw::PreferencesGroup`, or provide a narrow adapter rather than forcing all containers into one shape.

## Phase 0 — Baseline and migration inventory

### Tasks

- [ ] Record the current `waft-settings` UI entry points:
  - [ ] `src/main.rs`
  - [ ] `src/app.rs`
  - [ ] `src/window.rs`
  - [ ] `src/sidebar.rs`
  - [ ] `src/page_layout.rs`
  - [ ] `src/search_results.rs`
- [ ] Inventory every `src/pages/*.rs` page and classify it as:
  - [ ] static composition
  - [ ] stateful component
  - [ ] entity-driven dynamic page
  - [ ] direct KDL/configuration page
- [ ] Inventory every row, section, dialog, preview, and custom widget under:
  - [ ] `src/audio/`
  - [ ] `src/bluetooth/`
  - [ ] `src/display/`
  - [ ] `src/keyboard/`
  - [ ] `src/keyboard_shortcuts/`
  - [ ] `src/niri_windows/`
  - [ ] `src/notifications/`
  - [ ] `src/online_accounts/`
  - [ ] `src/scheduler/`
  - [ ] `src/services/`
  - [ ] `src/sounds/`
  - [ ] `src/startup/`
  - [ ] `src/wallpaper/`
  - [ ] `src/weather/`
  - [ ] `src/wifi/`
  - [ ] `src/wired/`
  - [ ] `src/plugins/`
- [ ] Identify all current `waft_ui_gtk::vdom` users in the crate.
- [ ] Identify all widgets that are already stored in maps and updated in place.
- [ ] Identify all locations with repeated layout builder code.
- [ ] Record current CSS classes and inline CSS in `src/app.rs`.
- [ ] Record all localization calls and ensure the migration does not introduce untranslated XML strings.
- [ ] Capture a baseline build/test/clippy result.
- [ ] Capture manual screenshots or a page-by-page smoke checklist for visual comparison.

### Exit criteria

- [ ] Every settings UI module has a migration classification.
- [ ] The first migration targets are selected based on stable structure and low behavioral risk.
- [ ] No daemon, protocol, DBus, or threading changes are included in the migration scope.

## Phase 1 — Establish the template/resource foundation

### Tasks

- [ ] Add the settings UI resource directory.
- [ ] Add the GResource manifest/build integration.
- [ ] Register resources exactly once during application startup.
- [ ] Add a minimal template-backed test widget or page.
- [ ] Verify template loading in debug and release builds.
- [ ] Add a convention for template IDs:
  - [ ] IDs use stable semantic names.
  - [ ] IDs do not encode transient entity IDs.
  - [ ] Every required ID has a corresponding Rust field or lookup check.
- [ ] Add a convention for XML files containing libadwaita widgets.
- [ ] Document how template parse/type errors are detected during tests or startup.
- [ ] Keep the existing CSS loading path working while CSS migration is staged.

### Exit criteria

- [ ] A composite-template widget loads from the registered resource.
- [ ] `cargo build --workspace` succeeds.
- [ ] The resource path works outside the source checkout after installation/packaging.

## Phase 2 — Convert the application shell

### Targets

- `src/window.rs`
- `src/sidebar.rs`
- `src/page_layout.rs`
- `src/search_results.rs`

### Tasks

- [ ] Define the static `AdwNavigationSplitView` shell in XML.
- [ ] Define static sidebar header/search presentation in XML.
- [ ] Define sidebar category/group containers in XML.
- [ ] Keep page category data and translated labels in Rust initially.
- [ ] Keep dynamic WiFi/wired sidebar visibility in Rust.
- [ ] Define the content navigation and page placeholder in XML.
- [ ] Keep the page factory/lazy construction behavior in Rust.
- [ ] Keep `gtk::Stack`/`AdwNavigationView` navigation callbacks in Rust.
- [ ] Convert the standard page root from repeated builder properties into a template or shared XML fragment only if that does not complicate ownership.
- [ ] Move stable search-result row structure into XML.
- [ ] Preserve search result selection and post-construction widget lookup behavior.
- [ ] Preserve initial-page command-line navigation.
- [ ] Preserve lazy page construction and page caching.

### Validation

- [ ] Sidebar categories render in the same order.
- [ ] Search opens, filters, and selects results correctly.
- [ ] WiFi and wired rows still appear/disappear based on entity state.
- [ ] Page navigation and sub-page back navigation work.
- [ ] `--page` still selects the requested page.

## Phase 3 — Convert reusable rows and controls

Start with small, stable widgets before converting whole pages.

### First targets

- [ ] `src/wifi/network_row.rs`
- [ ] `src/bluetooth/device_row.rs`
- [ ] `src/startup/startup_row.rs`
- [ ] `src/plugins/plugin_row.rs`
- [ ] `src/services/service_row.rs`
- [ ] `src/wired/connection_row.rs`
- [ ] `src/keyboard/layout_row.rs`
- [ ] `src/keyboard_shortcuts/bind_row.rs`
- [ ] `src/scheduler/timer_row.rs`
- [ ] `src/wallpaper/thumbnail_widget.rs`

### Per-component checklist

- [ ] Create the `.ui` template containing only stable structure.
- [ ] Create a Rust composite-template type.
- [ ] Replace builder-created child hierarchy with template children.
- [ ] Preserve existing props and output semantics.
- [ ] Add `update(&Props)` or equivalent in-place update API.
- [ ] Preserve signal handler behavior without accumulating duplicate handlers.
- [ ] Preserve keyboard activation and focus behavior.
- [ ] Preserve CSS classes and icon conventions.
- [ ] Preserve translated labels and subtitles.
- [ ] Add focused tests for state transitions where practical.
- [ ] Remove the old VDOM/builder implementation only after behavior matches.

### WiFi row pilot

- [ ] Create an XML-backed `NetworkRow` with:
  - [ ] `AdwActionRow`
  - [ ] signal-strength icon
  - [ ] security icon
  - [ ] connect/disconnect button
  - [ ] optional navigation chevron
- [ ] Keep signal icon selection in Rust.
- [ ] Toggle security icon and navigation chevron visibility from Rust.
- [ ] Update title, subtitle, button label, and sensitivity in place.
- [ ] Preserve `Connect` and `Disconnect` outputs.
- [ ] Preserve known-network navigation callbacks.
- [ ] Replace the VDOM `NetworkRow` with the template-backed widget.
- [ ] Update `KnownNetworksGroup` and `AvailableNetworksGroup` to store concrete GTK row widgets.
- [ ] Preserve URN-keyed add/update/remove behavior.
- [ ] Add deterministic child ordering.
- [ ] Verify repeated entity updates do not duplicate rows.
- [ ] Verify removed rows no longer receive callbacks.

## Phase 4 — Replace VDOM-backed dynamic lists with keyed GTK widgets

### Tasks

- [ ] Identify every settings component using `RenderComponent`, `VNode`, or `Reconciler`.
- [ ] For each component, classify whether its dynamic children are:
  - [ ] simple keyed rows
  - [ ] nested dynamic groups
  - [ ] animated/revealed content
  - [ ] a genuinely reusable VDOM tree
- [ ] Convert simple keyed rows to explicit GTK widget management.
- [ ] Reuse the keyed-child abstraction for add/update/remove/reorder.
- [ ] Keep stable widget identity across entity updates.
- [ ] Keep stable ordering independent of `HashMap` iteration order.
- [ ] Ensure removals happen on the GTK thread and do not invalidate active callbacks.
- [ ] Add coalescing/deferred reconciliation only where entity bursts make it necessary.
- [ ] Do not rebuild complete page trees for individual entity changes.
- [ ] Remove settings-only VDOM imports after each component is migrated.
- [ ] Decide whether any remaining VDOM functionality belongs in `waft-ui-gtk` or should be removed from the settings dependency.

### Dynamic-page targets

- [ ] WiFi adapters and network rows.
- [ ] Bluetooth adapters, paired devices, and discovered devices.
- [ ] Wired adapters and connection rows.
- [ ] Audio device cards and virtual devices.
- [ ] Online account rows and service toggles.
- [ ] Notification groups, profiles, and pattern rows.
- [ ] Plugin rows and system service rows.
- [ ] Scheduler timer rows.
- [ ] Wallpaper gallery thumbnails.
- [ ] Keyboard layouts and shortcut rows.
- [ ] Startup entries.

## Phase 5 — Convert static and mostly-static page sections

### Appearance/display

- [ ] `src/display/accent_colour_section.rs`
- [ ] `src/display/dark_mode_section.rs`
- [ ] `src/display/dark_mode_automation_section.rs`
- [ ] `src/display/night_light_section.rs`
- [ ] `src/display/night_light_config_section.rs`
- [ ] `src/display/output_section.rs`
- [ ] `src/display/settings_sub_page.rs`
- [ ] Keep entity values, toggles, automation schedules, and navigation callbacks in Rust.

### Audio/sounds

- [ ] `src/audio/device_card.rs`
- [ ] `src/audio/virtual_devices_section.rs`
- [ ] `src/sounds/defaults_section.rs`
- [ ] `src/sounds/gallery_section.rs`
- [ ] Keep device models, volume/mute updates, and action dispatch in Rust.

### Notifications

- [ ] `src/notifications/dnd_section.rs`
- [ ] `src/notifications/active_profile_section.rs`
- [ ] `src/notifications/recording_section.rs`
- [ ] `src/notifications/profiles_section.rs`
- [ ] `src/notifications/groups_section.rs`
- [ ] `src/notifications/group_form.rs`
- [ ] `src/notifications/combinator_editor.rs`
- [ ] `src/notifications/pattern_row.rs`
- [ ] Preserve incremental updates and avoid full-section rebuilds on every entity change.

### Niri window settings

- [ ] `src/niri_windows/focus_ring_section.rs`
- [ ] `src/niri_windows/border_section.rs`
- [ ] `src/niri_windows/shadow_section.rs`
- [ ] `src/niri_windows/tab_indicator_section.rs`
- [ ] `src/niri_windows/gaps_section.rs`
- [ ] `src/niri_windows/struts_section.rs`
- [ ] `src/niri_windows/derive_colors_section.rs`
- [ ] Keep KDL parsing, validation, and writes in Rust.

### Wallpaper/weather/keyboard

- [ ] `src/wallpaper/mode_section.rs`
- [ ] `src/wallpaper/config_section.rs`
- [ ] `src/wallpaper/preview_section.rs`
- [ ] `src/wallpaper/gallery_section.rs`
- [ ] `src/wallpaper/background_color_section.rs`
- [ ] `src/wallpaper/transition_section.rs`
- [ ] `src/weather/location_settings_group.rs`
- [ ] `src/weather/weather_preview_group.rs`
- [ ] `src/keyboard/keymap_grid.rs`
- [ ] `src/keyboard/variant_dialog.rs`
- [ ] `src/keyboard/add_layout_dialog.rs`
- [ ] `src/keyboard/rename_dialog.rs`
- [ ] Keep geocoding, XKB database access, weather requests, and entity actions in Rust.

## Phase 6 — Convert dialogs and sub-pages

### Targets

- [ ] WiFi password dialog.
- [ ] WiFi share dialog.
- [ ] WiFi network detail page.
- [ ] Online account add-account dialog.
- [ ] Startup entry dialog.
- [ ] Keyboard layout/variant/rename dialogs.
- [ ] Scheduler timer dialog and schedule picker.
- [ ] Settings sub-pages.

### Tasks

- [ ] Move stable dialog content hierarchy into XML.
- [ ] Keep dialog presentation, response handling, and validation in Rust.
- [ ] Keep destructive confirmation flows in Rust.
- [ ] Preserve default/cancel/destructive response appearance.
- [ ] Preserve focus, keyboard navigation, and entry activation.
- [ ] Ensure dialogs do not retain stale callbacks after their parent page is removed.
- [ ] Ensure template-backed dialogs can be presented repeatedly without duplicated signal handlers.

## Phase 7 — Convert the remaining page composers

### Page checklist

- [ ] Appearance.
- [ ] Audio.
- [ ] Bluetooth.
- [ ] Display.
- [ ] Keyboard.
- [ ] Keyboard Shortcuts.
- [ ] Niri Windows.
- [ ] Notifications.
- [ ] Online Accounts.
- [ ] Plugins.
- [ ] Power.
- [ ] Providers.
- [ ] Scheduler.
- [ ] Services.
- [ ] Sounds.
- [ ] Startup.
- [ ] Wallpaper.
- [ ] Weather.
- [ ] WiFi.
- [ ] Wired.

For each page:

- [ ] Define a template for the stable page hierarchy.
- [ ] Add placeholders for dynamic sections.
- [ ] Keep `register_search()` independent of widget construction.
- [ ] Keep search index backfilling after widgets exist.
- [ ] Keep entity subscriptions and initial reconciliation in Rust.
- [ ] Preserve lazy page construction from `SettingsWindow`.
- [ ] Preserve navigation-view references for sub-pages.
- [ ] Preserve page visibility and empty-state behavior.
- [ ] Preserve incremental UI updates and stable ordering.
- [ ] Compare the migrated page against the baseline screenshot/smoke checklist.

## Phase 8 — Migrate CSS and resource ownership

### Tasks

- [ ] Move settings-specific CSS from the inline raw string in `src/app.rs` into a CSS resource.
- [ ] Register CSS from the same resource-loading convention where practical.
- [ ] Preserve `.ordered-list`, `.ordered-list-row`, and all existing classes.
- [ ] Audit template classes against runtime classes to avoid duplicate styling responsibilities.
- [ ] Keep icon construction compliant with the project `IconWidget` convention.
- [ ] Verify dark/light theme rendering and libadwaita color variables.
- [ ] Verify high-contrast/accessibility behavior where supported.

## Phase 9 — Remove obsolete construction paths

### Tasks

- [ ] Remove obsolete page-level builder hierarchy code.
- [ ] Remove obsolete VDOM row implementations from `waft-settings`.
- [ ] Remove unused `waft_ui_gtk::vdom` imports and dependencies if no longer needed.
- [ ] Remove duplicated layout constants superseded by templates.
- [ ] Keep builders only for genuinely dynamic or transient objects where XML would reduce clarity.
- [ ] Remove dead callback adapters and conversion-only types.
- [ ] Update module documentation to describe template-backed components.
- [ ] Update `crates/settings/README.md` with the template/resource conventions.
- [ ] Add a short architecture note explaining the XML/Rust boundary.

## Phase 10 — Verification and regression coverage

### Automated validation

- [ ] `cargo fmt --all -- --check`
- [ ] `cargo check -p waft-settings`
- [ ] `cargo test -p waft-settings`
- [ ] `cargo clippy -p waft-settings --all-targets -- -D warnings`
- [ ] `cargo build --workspace`
- [ ] `cargo test --workspace`
- [ ] Verify installed/package-like execution can locate all UI resources.

### Behavioral validation

- [ ] Launch settings with no daemon entities available.
- [ ] Connect/reconnect to the daemon.
- [ ] Navigate to every sidebar page.
- [ ] Use settings search for pages, sections, and inputs.
- [ ] Open and close every migrated dialog repeatedly.
- [ ] Exercise page sub-navigation and back navigation.
- [ ] Toggle every migrated switch and combo row.
- [ ] Verify action success and error handling.
- [ ] Verify entity add/update/remove bursts.
- [ ] Verify rows are not duplicated after repeated updates.
- [ ] Verify removed rows no longer respond to actions.
- [ ] Verify stable ordering after updates.
- [ ] Verify empty-state descriptions.
- [ ] Verify keyboard navigation and activation.
- [ ] Verify light/dark themes.
- [ ] Verify localization in all supported locales.
- [ ] Verify KDL-backed pages still preserve safe load/save behavior.

### Manual smoke matrix

- [ ] Connectivity: Bluetooth, WiFi, Wired, Online Accounts.
- [ ] Visual: Appearance, Display, Windows, Wallpaper.
- [ ] Feedback: Audio, Notifications, Sounds.
- [ ] Inputs: Keyboard, Keyboard Shortcuts.
- [ ] Info: Weather.
- [ ] System: Power, Plugins, Providers, Services, Startup.
- [ ] Automation: Scheduled Tasks.

## Risks and mitigations

### XML/template runtime errors

Mitigation:

- centralize resources
- add startup/template smoke tests
- keep IDs stable and documented
- migrate one component at a time

### Stringly typed template child IDs

Mitigation:

- use `CompositeTemplate` fields for required children
- fail early for missing IDs
- avoid broad untyped lookup helpers

### VDOM and template lifecycle mismatch

Mitigation:

- migrate row types before dynamic lists
- replace VDOM list management with explicit keyed GTK management
- do not mix two owners for the same widget tree

### Lost incremental-update behavior

Mitigation:

- preserve existing `EntityStore` subscription boundaries
- require update-in-place behavior for keyed children
- add burst-update and duplicate-row tests

### Callback duplication or stale callbacks

Mitigation:

- connect handlers once during widget construction
- use guarded state/update methods for feedback loops
- remove rows and associated state together

### Localization drift

Mitigation:

- keep `t()` as the canonical translation path initially
- prohibit untranslated user-facing strings in XML
- audit every migrated component against existing Fluent keys

### Resource packaging failures

Mitigation:

- test debug, release, and installed-like execution
- include resource validation in CI
- keep the resource manifest close to the crate configuration

## Acceptance criteria

The migration is complete when:

1. All stable `waft-settings` widget hierarchies are defined by GTK XML templates or an explicitly documented exception.
2. Dynamic state, subscriptions, actions, navigation, and file I/O remain in Rust.
3. WiFi and other dynamic lists use explicit keyed GTK widget management with deterministic ordering.
4. No settings page depends on a VDOM abstraction merely to construct ordinary dynamic rows.
5. Entity updates modify existing widgets in place where identity is unchanged.
6. Entity removals remove widgets and associated callback/state ownership cleanly.
7. Search, lazy page construction, sub-pages, dialogs, localization, and command-line page navigation continue to work.
8. Existing visual behavior and CSS styling are preserved unless intentionally changed.
9. Resource loading works in debug, release, and installed-like execution.
10. Workspace build, tests, formatting, and clippy pass.
11. The settings README documents the XML/Rust boundary and template conventions.

## Recommended execution order

1. Baseline inventory.
2. Resource/template foundation.
3. WiFi `NetworkRow` pilot.
4. Explicit keyed GTK list management for WiFi.
5. Application shell and sidebar.
6. Other reusable rows.
7. Dynamic pages.
8. Static sections and dialogs.
9. CSS/resource cleanup.
10. VDOM removal and final validation.

This order keeps each migration reversible and ensures the first dynamic conversion exercises the exact architecture needed by the rest of the application.
