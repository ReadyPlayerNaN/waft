## ADDED Requirements

### Requirement: WiredAdapterWidget implements wired UI
The wired adapter UI SHALL be implemented using the WiredAdapterWidget architecture.

#### Scenario: WiredAdapterWidget owns toggle and menu
- **WHEN** a wired adapter is displayed
- **THEN** a WiredAdapterWidget instance SHALL manage the UI
- **AND** the widget SHALL own the FeatureToggleExpandableWidget
- **AND** the widget SHALL own the EthernetMenuWidget

#### Scenario: Event handlers are in WiredAdapterWidget
- **WHEN** the user interacts with the wired adapter UI
- **THEN** event handlers SHALL be methods in WiredAdapterWidget
- **AND** NOT inline closures in mod.rs

#### Scenario: State synchronization is in WiredAdapterWidget
- **WHEN** the wired adapter state changes
- **THEN** WiredAdapterWidget SHALL update the UI
- **AND** NOT mod.rs
