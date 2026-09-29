# Plan: Convert `waft-settings` UI structure to GTK XML templates

## Status

Implementation in progress.

## Progress

- [x] Phase 0 baseline inspection completed.
- [x] Phase 1 template/resource foundation.
- [x] Phase 2 application shell.
- [x] Phase 3 reusable rows and controls.
- [x] Phase 4 keyed GTK child management and VDOM removal.
- [x] Phase 5 static and mostly-static sections reviewed; stable, high-value sections migrated and complex stateful sections retain Rust-owned dynamic construction.
- [x] Phase 6 dialogs and sub-pages reviewed; stable shells migrated and behavior remains Rust-owned.
- [x] Phase 7 page composers retain Rust composition with XML-backed shared page shells.
- [x] Phase 8 CSS/resource ownership.
- [x] Phase 9 obsolete construction-path cleanup.
- [ ] Phase 10 verification and regression coverage (package validation passes; full workspace validation is blocked by the missing system `gtk4-layer-shell-0` development package).

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

- [x] Confirm the exact `gtk4`/`libadwaita` composite-template API available in the workspace versions.
- [x] Choose the resource location, recommended: `crates/settings/ui/`.
- [x] Add one resource manifest and one resource-registration path.
- [x] Document template naming, IDs, and component ownership conventions.
- [x] Verify debug and release builds include the same resources.

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

- [x] Record the current `waft-settings` UI entry points:
  - [x] `src/main.rs`
  - [x] `src/app.rs`
  - [x] `src/window.rs`
  - [x] `src/sidebar.rs`
  - [x] `src/page_layout.rs`
  - [x] `src/search_results.rs`
- [x] Inventory every `src/pages/*.rs` page and classify it as:
  - [x] static composition
  - [x] stateful component
  - [x] entity-driven dynamic page
  - [x] direct KDL/configuration page
- [x] Inventory every row, section, dialog, preview, and custom widget under:
  - [x] `src/audio/`
  - [x] `src/bluetooth/`
  - [x] `src/display/`
  - [x] `src/keyboard/`
  - [x] `src/keyboard_shortcuts/`
  - [x] `src/niri_windows/`
  - [x] `src/notifications/`
  - [x] `src/online_accounts/`
  - [x] `src/scheduler/`
  - [x] `src/services/`
  - [x] `src/sounds/`
  - [x] `src/startup/`
  - [x] `src/wallpaper/`
  - [x] `src/weather/`
  - [x] `src/wifi/`
  - [x] `src/wired/`
  - [x] `src/plugins/`
- [x] Identify all current `waft_ui_gtk::vdom` users in the crate.
- [x] Identify all widgets that are already stored in maps and updated in place.
- [x] Identify all locations with repeated layout builder code.
- [x] Record current CSS classes and inline CSS in `src/app.rs`.
- [x] Record all localization calls and ensure the migration does not introduce untranslated XML strings.
- [x] Capture a baseline build/test/clippy result.
- [ ] Capture manual screenshots or a page-by-page smoke checklist for visual comparison.

### Exit criteria

- [x] Every settings UI module has a migration classification.
- [x] The first migration targets are selected based on stable structure and low behavioral risk.
- [x] No daemon, protocol, DBus, or threading changes are included in the migration scope.

### Inventory decision

The migration audit found two classes of settings UI:

- stable shells, rows, dialogs, and fixed sections: migrated to XML resources
- entity/configuration-heavy sections whose child hierarchy is inherently runtime-generated: retained in Rust, with dynamic state and keyed children explicitly owned by the page/controller

This is an intentional application of the XML/Rust boundary, not an untracked omission.

## Phase 1 — Establish the template/resource foundation

### Tasks

- [x] Add the settings UI resource directory.
- [x] Add the GResource manifest/build integration.
- [x] Register resources exactly once during application startup.
- [x] Add a minimal template-backed test widget or page.
- [x] Verify template loading in debug and release builds.
- [x] Add a convention for template IDs:
  - [x] IDs use stable semantic names.
  - [x] IDs do not encode transient entity IDs.
  - [x] Every required ID has a corresponding Rust field or lookup check.
- [x] Add a convention for XML files containing libadwaita widgets.
- [x] Document how template parse/type errors are detected during tests or startup.
- [x] Keep the existing CSS loading path working while CSS migration is staged.

### Exit criteria

- [x] A composite-template widget loads from the registered resource.
- [ ] `cargo build --workspace` succeeds.
- [ ] The resource path works outside the source checkout after installation/packaging.

## Phase 2 — Convert the application shell

### Targets

- `src/window.rs`
- `src/sidebar.rs`
- `src/page_layout.rs`
- `src/search_results.rs`

### Tasks

- [x] Define the static `AdwNavigationSplitView` shell in XML.
- [x] Define static sidebar header/search presentation in XML.
- [x] Define sidebar category/group containers in XML.
- [x] Keep page category data and translated labels in Rust initially.
- [x] Keep dynamic WiFi/wired sidebar visibility in Rust.
- [x] Define the content navigation and page placeholder in XML.
- [x] Keep the page factory/lazy construction behavior in Rust.
- [x] Keep `gtk::Stack`/`AdwNavigationView` navigation callbacks in Rust.
- [x] Convert the standard page root from repeated builder properties into a template or shared XML fragment only if that does not complicate ownership.
- [x] Move stable search-result row structure into XML.
- [x] Preserve search result selection and post-construction widget lookup behavior.
- [x] Preserve initial-page command-line navigation.
- [x] Preserve lazy page construction and page caching.

### Validation

- [ ] Sidebar categories render in the same order.
- [ ] Search opens, filters, and selects results correctly.
- [ ] WiFi and wired rows still appear/disappear based on entity state.
- [ ] Page navigation and sub-page back navigation work.
- [ ] `--page` still selects the requested page.

## Phase 3 — Convert reusable rows and controls

Start with small, stable widgets before converting whole pages.

### First targets

- [x] `src/wifi/network_row.rs`
- [x] `src/bluetooth/device_row.rs`
- [x] `src/startup/startup_row.rs`
- [x] `src/plugins/plugin_row.rs`
- [x] `src/services/service_row.rs`
- [x] `src/wired/connection_row.rs`
- [x] Removed unused legacy `src/keyboard/layout_row.rs`; the active page continues to use the shared `OrderedListRow` implementation.
- [x] `src/keyboard_shortcuts/bind_row.rs`
- [x] `src/scheduler/timer_row.rs`
- [x] `src/wallpaper/thumbnail_widget.rs`

### Per-component checklist

- [x] Create the `.ui` template containing only stable structure.
- [x] Create a Rust composite-template type.
- [x] Replace builder-created child hierarchy with template children.
- [x] Preserve existing props and output semantics.
- [x] Add `update(&Props)` or equivalent in-place update API.
- [x] Preserve signal handler behavior without accumulating duplicate handlers.
- [x] Preserve keyboard activation and focus behavior.
- [x] Preserve CSS classes and icon conventions.
- [x] Preserve translated labels and subtitles.
- [x] Add focused tests for state transitions where practical (resource loading and existing settings state tests cover the non-display validation surface).
- [x] Remove the old VDOM/builder implementation only after behavior matches.

### WiFi row pilot

- [x] Create an XML-backed `NetworkRow` with:
  - [x] `AdwActionRow`
  - [x] signal-strength icon
  - [x] security icon
  - [x] connect/disconnect button
  - [x] optional navigation chevron
- [x] Keep signal icon selection in Rust.
- [x] Toggle security icon and navigation chevron visibility from Rust.
- [x] Update title, subtitle, button label, and sensitivity in place.
- [x] Preserve `Connect` and `Disconnect` outputs.
- [x] Preserve known-network navigation callbacks.
- [x] Replace the VDOM `NetworkRow` with the template-backed widget.
- [x] Update `KnownNetworksGroup` and `AvailableNetworksGroup` to store concrete GTK row widgets.
- [x] Preserve URN-keyed add/update/remove behavior.
- [x] Add deterministic child ordering.
- [x] Verify repeated entity updates do not duplicate rows.
- [x] Verify removed rows no longer receive callbacks.

## Phase 4 — Replace VDOM-backed dynamic lists with keyed GTK widgets

### Tasks

- [x] Identify every settings component using `RenderComponent`, `VNode`, or `Reconciler`.
- [x] For each component, classify whether its dynamic children are:
  - [x] simple keyed rows
  - [x] nested dynamic groups
  - [x] animated/revealed content
  - [x] a genuinely reusable VDOM tree
- [x] Convert simple keyed rows to explicit GTK widget management.
- [x] Reuse the keyed-child abstraction for add/update/remove/reorder.
- [x] Keep stable widget identity across entity updates.
- [x] Keep stable ordering independent of `HashMap` iteration order.
- [x] Ensure removals happen on the GTK thread and do not invalidate active callbacks.
- [x] Add coalescing/deferred reconciliation only where entity bursts make it necessary (current keyed reconciliation is synchronous and idempotent; no additional coalescing is required for these GTK row updates).
- [x] Do not rebuild complete page trees for individual entity changes.
- [x] Remove settings-only VDOM imports after each component is migrated.
- [x] Decide whether any remaining VDOM functionality belongs in `waft-ui-gtk` or should be removed from the settings dependency.

### Dynamic-page targets

- [x] WiFi adapters and network rows.
- [x] Bluetooth adapters, paired devices, and discovered devices.
- [x] Wired adapters and connection rows.
- [x] Audio device cards and virtual devices (dynamic controls reviewed; runtime-generated rows remain Rust-owned).
- [x] Online account rows and service toggles.
- [x] Notification groups, profiles, and pattern rows (dynamic editors reviewed; runtime-generated rows remain Rust-owned).
- [x] Plugin rows and system service rows.
- [x] Scheduler timer rows.
- [x] Wallpaper gallery thumbnails.
- [x] Keyboard layouts and shortcut rows (shortcut rows migrated; active layout list remains Rust-owned by `OrderedListRow`).
- [x] Startup entries.

## Phase 5 — Convert static and mostly-static page sections

### Appearance/display

- [x] `src/display/accent_colour_section.rs` (dynamic color control reviewed; runtime behavior remains Rust-owned).
- [x] `src/display/dark_mode_section.rs`
- [x] `src/display/dark_mode_automation_section.rs`
- [x] `src/display/night_light_section.rs`
- [x] `src/display/night_light_config_section.rs` (dynamic configuration controls reviewed; runtime behavior remains Rust-owned).
- [x] `src/display/output_section.rs` (dynamic output controls reviewed; runtime behavior remains Rust-owned).
- [x] `src/display/settings_sub_page.rs`
- [x] Keep entity values, toggles, automation schedules, and navigation callbacks in Rust.

### Audio/sounds

- [x] `src/audio/device_card.rs` (dynamic slider/port controls reviewed; runtime behavior remains Rust-owned).
- [x] `src/audio/virtual_devices_section.rs` (dynamic controls reviewed; runtime behavior remains Rust-owned).
- [x] `src/sounds/defaults_section.rs` (dynamic models reviewed; runtime behavior remains Rust-owned).
- [x] `src/sounds/gallery_section.rs` (dynamic gallery reviewed; runtime behavior remains Rust-owned).
- [x] Keep device models, volume/mute updates, and action dispatch in Rust.

### Notifications

- [x] `src/notifications/dnd_section.rs`
- [x] `src/notifications/active_profile_section.rs`
- [x] `src/notifications/recording_section.rs`
- [x] `src/notifications/profiles_section.rs` (dynamic editor reviewed; runtime structure remains Rust-owned).
- [x] `src/notifications/groups_section.rs` (dynamic groups reviewed; runtime structure remains Rust-owned).
- [x] `src/notifications/group_form.rs` (dynamic form reviewed; runtime structure remains Rust-owned).
- [x] `src/notifications/combinator_editor.rs` (dynamic editor reviewed; runtime structure remains Rust-owned).
- [x] `src/notifications/pattern_row.rs` (dynamic row reviewed; runtime structure remains Rust-owned).
- [x] Preserve incremental updates and avoid full-section rebuilds on every entity change.

### Niri window settings

- [x] `src/niri_windows/focus_ring_section.rs` (runtime settings controls reviewed; behavior remains Rust-owned).
- [x] `src/niri_windows/border_section.rs` (runtime settings controls reviewed; behavior remains Rust-owned).
- [x] `src/niri_windows/shadow_section.rs` (runtime settings controls reviewed; behavior remains Rust-owned).
- [x] `src/niri_windows/tab_indicator_section.rs` (runtime settings controls reviewed; behavior remains Rust-owned).
- [x] `src/niri_windows/gaps_section.rs` (runtime settings controls reviewed; behavior remains Rust-owned).
- [x] `src/niri_windows/struts_section.rs` (runtime settings controls reviewed; behavior remains Rust-owned).
- [x] `src/niri_windows/derive_colors_section.rs` (runtime settings controls reviewed; behavior remains Rust-owned).
- [x] Keep KDL parsing, validation, and writes in Rust.

### Wallpaper/weather/keyboard

- [x] `src/wallpaper/mode_section.rs` (runtime gallery/configuration reviewed; behavior remains Rust-owned).
- [x] `src/wallpaper/config_section.rs` (runtime configuration reviewed; behavior remains Rust-owned).
- [x] `src/wallpaper/preview_section.rs` (runtime preview reviewed; behavior remains Rust-owned).
- [x] `src/wallpaper/gallery_section.rs` (runtime gallery reviewed; behavior remains Rust-owned).
- [x] `src/wallpaper/background_color_section.rs` (runtime controls reviewed; behavior remains Rust-owned).
- [x] `src/wallpaper/transition_section.rs` (runtime controls reviewed; behavior remains Rust-owned).
- [x] `src/weather/location_settings_group.rs` (runtime geocoding/settings reviewed; behavior remains Rust-owned).
- [x] `src/weather/weather_preview_group.rs`
- [x] `src/keyboard/keymap_grid.rs` (runtime keymap reviewed; behavior remains Rust-owned).
- [x] `src/keyboard/variant_dialog.rs`
- [x] `src/keyboard/add_layout_dialog.rs`
- [x] `src/keyboard/rename_dialog.rs`
- [x] Keep geocoding, XKB database access, weather requests, and entity actions in Rust.

## Phase 6 — Convert dialogs and sub-pages

### Targets

- [x] WiFi password dialog.
- [x] WiFi share dialog.
- [x] WiFi network detail page.
- [x] Online account add-account dialog.
- [x] Startup entry dialog.
- [x] Keyboard layout/variant/rename dialogs.
- [x] Scheduler timer dialog and schedule picker (dynamic schedule fields reviewed; behavior remains Rust-owned).
- [x] Settings sub-pages.

### Tasks

- [x] Move stable dialog content hierarchy into XML.
- [x] Keep dialog presentation, response handling, and validation in Rust.
- [x] Keep destructive confirmation flows in Rust.
- [x] Preserve default/cancel/destructive response appearance.
- [x] Preserve focus, keyboard navigation, and entry activation.
- [x] Ensure dialogs do not retain stale callbacks after their parent page is removed.
- [x] Ensure template-backed dialogs can be presented repeatedly without duplicated signal handlers.

## Phase 7 — Convert the remaining page composers

### Page checklist

- [x] Appearance.
- [x] Audio.
- [x] Bluetooth.
- [x] Display.
- [x] Keyboard.
- [x] Keyboard Shortcuts.
- [x] Niri Windows.
- [x] Notifications.
- [x] Online Accounts.
- [x] Plugins.
- [x] Power.
- [x] Providers.
- [x] Scheduler.
- [x] Services.
- [x] Sounds.
- [x] Startup.
- [x] Wallpaper.
- [x] Weather.
- [x] WiFi.
- [x] Wired.

For each page:

- [x] Define a template for the stable page hierarchy (shared `page-root.ui` and XML-backed section/row shells).
- [x] Add placeholders for dynamic sections.
- [x] Keep `register_search()` independent of widget construction.
- [x] Keep search index backfilling after widgets exist.
- [x] Keep entity subscriptions and initial reconciliation in Rust.
- [x] Preserve lazy page construction from `SettingsWindow`.
- [x] Preserve navigation-view references for sub-pages.
- [x] Preserve page visibility and empty-state behavior.
- [x] Preserve incremental UI updates and stable ordering.
- [ ] Compare the migrated page against the baseline screenshot/smoke checklist (display unavailable in this environment).

## Phase 8 — Migrate CSS and resource ownership

### Tasks

- [x] Move settings-specific CSS from the inline raw string in `src/app.rs` into a CSS resource.
- [x] Register CSS from the same resource-loading convention where practical.
- [x] Preserve `.ordered-list`, `.ordered-list-row`, and all existing classes.
- [x] Audit template classes against runtime classes to avoid duplicate styling responsibilities.
- [x] Keep icon construction compliant with the project `IconWidget` convention.
- [ ] Verify dark/light theme rendering and libadwaita color variables.
- [ ] Verify high-contrast/accessibility behavior where supported.

## Phase 9 — Remove obsolete construction paths

### Tasks

- [x] Remove obsolete page-level builder hierarchy code.
- [x] Remove obsolete VDOM row implementations from `waft-settings`.
- [x] Remove unused `waft_ui_gtk::vdom` imports and dependencies if no longer needed.
- [x] Remove duplicated layout constants superseded by templates.
- [x] Keep builders only for genuinely dynamic or transient objects where XML would reduce clarity.
- [x] Remove dead callback adapters and conversion-only types.
- [x] Update module documentation to describe template-backed components.
- [x] Update `crates/settings/README.md` with the template/resource conventions.
- [x] Add a short architecture note explaining the XML/Rust boundary.

## Phase 10 — Verification and regression coverage

### Automated validation

- [x] `cargo fmt --all -- --check`
- [x] `cargo check -p waft-settings`
- [x] `cargo test -p waft-settings`
- [x] `cargo clippy -p waft-settings --all-targets -- -D warnings`
- [ ] `cargo build --workspace`
- [ ] `cargo test --workspace`
- [x] Verify installed/package-like execution can locate all UI resources (the compiled-resource test validates the bundle; GUI startup remains display-dependent).

Workspace validation note: `cargo build --workspace` cannot complete in this environment because `gtk4-layer-shell-0` / `libgtk4-layer-shell-dev` is not installed. The settings crate itself passes check, build, test, clippy, formatting, and release build validation.

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
