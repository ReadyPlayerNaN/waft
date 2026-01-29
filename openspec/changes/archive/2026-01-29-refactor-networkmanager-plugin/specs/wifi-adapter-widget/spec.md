## ADDED Requirements

### Requirement: WiFiAdapterWidget encapsulates WiFi adapter UI logic
The system SHALL provide a WiFiAdapterWidget that encapsulates all UI logic for a single WiFi adapter.

#### Scenario: Widget owns toggle and menu components
- **WHEN** a WiFiAdapterWidget is created
- **THEN** it SHALL own a FeatureToggleExpandableWidget instance
- **AND** it SHALL own a WiFiMenuWidget instance

#### Scenario: Widget manages its own event handlers
- **WHEN** a WiFiAdapterWidget is initialized
- **THEN** it SHALL set up all toggle event handlers internally
- **AND** it SHALL set up all menu event handlers internally
- **AND** it SHALL set up expand callbacks internally

#### Scenario: Widget exposes a single widget reference
- **WHEN** the plugin needs to register the adapter widget
- **THEN** WiFiAdapterWidget SHALL provide a widget() method
- **AND** the method SHALL return an Arc<WidgetFeatureToggle> for registration

### Requirement: WiFiAdapterWidget handles toggle events
The system SHALL handle WiFi adapter toggle events within the WiFiAdapterWidget.

#### Scenario: Toggle activation enables WiFi
- **WHEN** the user activates the WiFi adapter toggle
- **THEN** the widget SHALL call dbus::set_wifi_enabled_nmrs() with enabled=true
- **AND** it SHALL use the NetworkManager instance
- **AND** it SHALL emit SetWiFiBusy operation to the store
- **AND** it SHALL handle the operation asynchronously on a separate thread

#### Scenario: Toggle deactivation disables WiFi
- **WHEN** the user deactivates the WiFi adapter toggle
- **THEN** the widget SHALL call dbus::set_wifi_enabled_nmrs() with enabled=false
- **AND** it SHALL use the NetworkManager instance
- **AND** it SHALL emit SetWiFiBusy operation to the store
- **AND** it SHALL handle the operation asynchronously on a separate thread

#### Scenario: Toggle completion updates store
- **WHEN** the WiFi enable/disable operation completes
- **THEN** the widget SHALL emit SetWiFiEnabled operation to the store
- **AND** it SHALL emit SetWiFiBusy(false) operation to the store

### Requirement: WiFiAdapterWidget handles menu expand with auto-scan
The system SHALL automatically scan for networks when the WiFi menu is expanded.

#### Scenario: Menu expand triggers network scan
- **WHEN** the user expands the WiFi adapter toggle
- **THEN** the widget SHALL call dbus::scan_networks_nmrs()
- **AND** it SHALL wait 3 seconds for scan completion
- **AND** it SHALL call dbus::list_networks_nmrs() to get access points

#### Scenario: Scan results are filtered and deduplicated
- **WHEN** access points are retrieved after scan
- **THEN** the widget SHALL filter out networks without saved profiles
- **AND** it SHALL deduplicate by SSID keeping strongest signal
- **AND** it SHALL emit SetWiFiAccessPoints operation to the store

#### Scenario: Menu displays scanned networks
- **WHEN** scan results are available
- **THEN** the widget SHALL update the WiFiMenuWidget with networks
- **AND** it SHALL update toggle details with network count or active SSID

### Requirement: WiFiAdapterWidget handles network connection
The system SHALL handle WiFi network connection requests within the WiFiAdapterWidget.

#### Scenario: User selects network to connect
- **WHEN** the user clicks a network in the WiFi menu
- **THEN** the widget SHALL set the network as "connecting" in the menu
- **AND** it SHALL call dbus::get_connections_for_ssid() to find saved profiles
- **AND** it SHALL call dbus::activate_connection() with the connection path

#### Scenario: Connection success updates state
- **WHEN** the connection activation succeeds
- **THEN** the widget SHALL emit SetActiveWiFiConnection operation to the store
- **AND** it SHALL update the toggle details to show the SSID
- **AND** it SHALL update the menu to show the network as active
- **AND** it SHALL clear the "connecting" state

#### Scenario: Connection failure clears connecting state
- **WHEN** the connection activation fails
- **THEN** the widget SHALL log the error
- **AND** it SHALL clear the "connecting" state in the menu

### Requirement: WiFiAdapterWidget synchronizes state from store
The system SHALL synchronize WiFiAdapterWidget UI with NetworkStore state.

#### Scenario: Widget subscribes to store changes
- **WHEN** a WiFiAdapterWidget is created
- **THEN** it SHALL subscribe to NetworkStore state changes
- **AND** it SHALL update UI when the adapter state changes

#### Scenario: State change updates toggle properties
- **WHEN** the adapter state changes in the store
- **THEN** the widget SHALL update toggle enabled state
- **AND** it SHALL update toggle busy state
- **AND** it SHALL update toggle details with active SSID or network count

#### Scenario: State change updates menu
- **WHEN** the adapter access points change in the store
- **THEN** the widget SHALL update the WiFiMenuWidget with new networks
- **AND** it SHALL update the active SSID in the menu

### Requirement: WiFiAdapterWidget uses async thread + channel pattern
The system SHALL use the established async pattern for D-Bus operations in WiFiAdapterWidget.

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

### Requirement: WiFiAdapterWidget reduces cognitive complexity
The system SHALL structure WiFiAdapterWidget to reduce cognitive complexity compared to mod.rs.

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
