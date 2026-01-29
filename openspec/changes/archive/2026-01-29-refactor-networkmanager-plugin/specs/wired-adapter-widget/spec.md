## ADDED Requirements

### Requirement: WiredAdapterWidget encapsulates wired adapter UI logic
The system SHALL provide a WiredAdapterWidget that encapsulates all UI logic for a single wired ethernet adapter.

#### Scenario: Widget owns toggle and menu components
- **WHEN** a WiredAdapterWidget is created
- **THEN** it SHALL own a FeatureToggleExpandableWidget instance
- **AND** it SHALL own an EthernetMenuWidget instance

#### Scenario: Widget manages its own event handlers
- **WHEN** a WiredAdapterWidget is initialized
- **THEN** it SHALL set up all toggle event handlers internally
- **AND** it SHALL set up all menu event handlers internally
- **AND** it SHALL set up expand callbacks internally

#### Scenario: Widget exposes a single widget reference
- **WHEN** the plugin needs to register the adapter widget
- **THEN** WiredAdapterWidget SHALL provide a widget() method
- **AND** the method SHALL return an Arc<WidgetFeatureToggle> for registration

### Requirement: WiredAdapterWidget handles toggle events
The system SHALL handle wired adapter toggle events within the WiredAdapterWidget.

#### Scenario: Toggle activation triggers connection
- **WHEN** the user activates the wired adapter toggle
- **THEN** the widget SHALL call dbus::connect_wired_nmrs()
- **AND** it SHALL use the NetworkManager instance
- **AND** it SHALL handle the operation asynchronously on a separate thread

#### Scenario: Toggle deactivation triggers disconnection
- **WHEN** the user deactivates the wired adapter toggle
- **THEN** the widget SHALL call dbus::disconnect_nmrs()
- **AND** it SHALL use the NetworkManager instance
- **AND** it SHALL handle the operation asynchronously on a separate thread

#### Scenario: Toggle expand triggers detail fetch
- **WHEN** the user expands the wired adapter toggle
- **THEN** the widget SHALL fetch connection details asynchronously
- **AND** it SHALL update the EthernetMenuWidget with the details

### Requirement: WiredAdapterWidget synchronizes state from store
The system SHALL synchronize WiredAdapterWidget UI with NetworkStore state.

#### Scenario: Widget subscribes to store changes
- **WHEN** a WiredAdapterWidget is created
- **THEN** it SHALL subscribe to NetworkStore state changes
- **AND** it SHALL update UI when the adapter state changes

#### Scenario: State change updates toggle properties
- **WHEN** the adapter state changes in the store
- **THEN** the widget SHALL update toggle enabled state
- **AND** it SHALL update toggle icon based on connection state
- **AND** it SHALL update toggle details text based on connection state

#### Scenario: Connection state updates icon
- **WHEN** the adapter state shows device_state == 100 (connected)
- **THEN** the toggle icon SHALL be "network-wired-symbolic"

#### Scenario: Disconnected state updates icon
- **WHEN** the adapter state shows device_state < 100 and carrier is false
- **THEN** the toggle icon SHALL be "network-wired-disconnected-symbolic"

#### Scenario: Disabled state updates icon
- **WHEN** the adapter enabled is false
- **THEN** the toggle icon SHALL be "network-wired-offline-symbolic"

### Requirement: WiredAdapterWidget uses async thread + channel pattern
The system SHALL use the established async pattern for D-Bus operations in WiredAdapterWidget.

#### Scenario: D-Bus operations spawn separate threads
- **WHEN** the widget needs to perform a D-Bus operation
- **THEN** it SHALL spawn a std::thread
- **AND** the thread SHALL create a tokio runtime
- **AND** the thread SHALL execute the async operation
- **AND** the thread SHALL send results via std::sync::mpsc channel

#### Scenario: Results are polled on glib main loop
- **WHEN** a D-Bus operation is in progress
- **THEN** the widget SHALL use glib::timeout_add_local to poll the channel
- **AND** it SHALL update UI on the main thread when results arrive
- **AND** it SHALL handle Empty, Ok, and Disconnected channel states

### Requirement: WiredAdapterWidget reduces cognitive complexity
The system SHALL structure WiredAdapterWidget to reduce cognitive complexity compared to mod.rs.

#### Scenario: Event handlers are named methods
- **WHEN** an event occurs (toggle, menu, expand)
- **THEN** the handler SHALL be a named method on the widget
- **AND** NOT an inline closure

#### Scenario: Maximum nesting depth is limited
- **WHEN** implementing widget methods
- **THEN** nesting depth SHALL NOT exceed 3 levels
- **AND** complex operations SHALL be extracted into helper methods

#### Scenario: Each method has a single responsibility
- **WHEN** implementing widget methods
- **THEN** each method SHALL have one clear purpose
- **AND** methods longer than 50 lines SHALL be refactored into smaller methods
