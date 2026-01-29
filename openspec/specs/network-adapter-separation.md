## ADDED Requirements

### Requirement: Plugin coordinator pattern separates concerns
The system SHALL separate plugin coordination logic from adapter-specific logic.

#### Scenario: mod.rs acts as plugin coordinator only
- **WHEN** the NetworkManagerPlugin is implemented
- **THEN** mod.rs SHALL handle plugin initialization
- **AND** mod.rs SHALL handle device discovery
- **AND** mod.rs SHALL handle widget registration
- **AND** mod.rs SHALL NOT contain adapter-specific event handlers

#### Scenario: Adapter widgets are self-contained
- **WHEN** an adapter widget is created
- **THEN** it SHALL contain all logic for that adapter type
- **AND** it SHALL NOT require external coordination for events
- **AND** it SHALL interact with the store independently

#### Scenario: mod.rs is under 200 lines
- **WHEN** the refactoring is complete
- **THEN** mod.rs SHALL be less than 200 lines of code
- **AND** it SHALL primarily contain structural code
- **AND** it SHALL NOT contain inline event handlers longer than 5 lines

### Requirement: Adapter widgets follow consistent architecture
The system SHALL implement all adapter widgets with a consistent architectural pattern.

#### Scenario: All adapter widgets have the same structure
- **WHEN** implementing WiredAdapterWidget, WiFiAdapterWidget, or VpnAdapterWidget
- **THEN** each SHALL have a new() constructor taking adapter state, store, nm, dbus, and menu_store
- **AND** each SHALL have a widget() method returning Arc<WidgetFeatureToggle>
- **AND** each SHALL have a sync_state() method for store synchronization
- **AND** each SHALL own its toggle and menu components

#### Scenario: All adapter widgets use the same async pattern
- **WHEN** adapter widgets perform D-Bus operations
- **THEN** all SHALL use std::thread spawning with tokio runtime
- **AND** all SHALL use std::sync::mpsc channels for results
- **AND** all SHALL use glib::timeout_add_local for polling

#### Scenario: All adapter widgets emit store operations
- **WHEN** adapter widgets need to update state
- **THEN** all SHALL emit operations to NetworkStore
- **AND** all SHALL NOT directly mutate state
- **AND** all SHALL react to state changes via store subscription

### Requirement: Clear separation between UI and business logic
The system SHALL maintain clear separation between UI components and business logic.

#### Scenario: Toggle widgets are presentational only
- **WHEN** implementing WiredToggleWidget or WiFiToggleWidget
- **THEN** they SHALL only handle UI presentation
- **AND** they SHALL NOT contain business logic
- **AND** they SHALL NOT make D-Bus calls

#### Scenario: Adapter widgets contain business logic
- **WHEN** implementing WiredAdapterWidget or WiFiAdapterWidget
- **THEN** they SHALL contain business logic for adapter operations
- **AND** they SHALL coordinate between toggle, menu, and D-Bus
- **AND** they SHALL handle state synchronization

#### Scenario: Menu widgets remain unchanged
- **WHEN** refactoring to adapter widgets
- **THEN** EthernetMenuWidget and WiFiMenuWidget SHALL NOT be modified
- **AND** they SHALL continue to be presentational components
- **AND** they SHALL be owned by adapter widgets

### Requirement: File organization reflects architectural separation
The system SHALL organize files to reflect the separation of concerns.

#### Scenario: One file per adapter widget
- **WHEN** the refactoring is complete
- **THEN** there SHALL be a wired_adapter_widget.rs file
- **AND** there SHALL be a wifi_adapter_widget.rs file
- **AND** each SHALL contain only code for that adapter type

#### Scenario: One file per toggle widget
- **WHEN** creating toggle widgets
- **THEN** there SHALL be a wired_toggle_widget.rs file (if implemented)
- **AND** there SHALL be a wifi_toggle.rs file (existing)
- **AND** each SHALL contain only presentational toggle code

#### Scenario: mod.rs imports and uses adapter widgets
- **WHEN** mod.rs creates adapters
- **THEN** it SHALL import WiredAdapterWidget and WiFiAdapterWidget
- **AND** it SHALL create adapter widget instances
- **AND** it SHALL register their widgets via get_feature_toggles()

### Requirement: Maintain existing functionality during refactoring
The system SHALL maintain all existing NetworkManager functionality during the refactoring.

#### Scenario: All user-facing features work identically
- **WHEN** the refactoring is complete
- **THEN** all wired adapter features SHALL work as before
- **AND** all WiFi adapter features SHALL work as before
- **AND** all menu features SHALL work as before
- **AND** all state synchronization SHALL work as before

#### Scenario: Store operations remain unchanged
- **WHEN** adapter widgets emit operations
- **THEN** they SHALL use the same NetworkOp types as before
- **AND** they SHALL emit operations in the same circumstances as before
- **AND** the NetworkStore SHALL receive identical operations

#### Scenario: No breaking changes to Plugin trait
- **WHEN** the refactoring is complete
- **THEN** the Plugin trait implementation SHALL remain unchanged
- **AND** the public interface SHALL remain unchanged
- **AND** external code depending on the plugin SHALL NOT break
