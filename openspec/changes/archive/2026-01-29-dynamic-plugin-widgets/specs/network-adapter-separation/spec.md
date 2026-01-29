## MODIFIED Requirements

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

#### Scenario: Plugin trait implementation uses WidgetRegistrar
- **WHEN** the refactoring is complete
- **THEN** the Plugin trait implementation SHALL use WidgetRegistrar for widget registration
- **AND** the `create_elements()` method SHALL accept the registrar parameter
- **AND** adapter widgets SHALL be registered dynamically

## ADDED Requirements

### Requirement: Adapter widgets register dynamically based on hardware state
The system SHALL register and unregister adapter widgets when hardware state changes.

#### Scenario: Adapter widget registered when adapter appears
- **WHEN** NetworkManager reports a new network adapter (wired or WiFi)
- **THEN** the plugin SHALL create the corresponding adapter widget
- **AND** the plugin SHALL call `registrar.register_feature_toggle()` for the widget
- **AND** the main window SHALL display the new toggle

#### Scenario: Adapter widget unregistered when adapter disappears
- **WHEN** NetworkManager reports an adapter has been removed (e.g., USB unplugged)
- **THEN** the plugin SHALL call `registrar.unregister_feature_toggle(id)` for that adapter
- **AND** the main window SHALL remove the toggle from display
- **AND** the widget resources SHALL be cleaned up

#### Scenario: Multiple adapters of same type supported
- **WHEN** multiple adapters of the same type exist (e.g., two WiFi adapters)
- **THEN** each SHALL have a unique widget ID (e.g., `networkmanager:wifi:adapter-0`, `networkmanager:wifi:adapter-1`)
- **AND** each SHALL be independently registered and displayed
