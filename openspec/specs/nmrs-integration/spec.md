## ADDED Requirements

### Requirement: Use nmrs for NetworkManager availability check
The networkmanager plugin SHALL use the `nmrs` crate to check NetworkManager availability instead of manual D-Bus method calls.

#### Scenario: NetworkManager is available
- **WHEN** the plugin initializes and NetworkManager is running on the system bus
- **THEN** the availability check SHALL succeed using nmrs API

#### Scenario: NetworkManager is not available
- **WHEN** the plugin initializes and NetworkManager is not running
- **THEN** the availability check SHALL fail gracefully using nmrs API

### Requirement: Use nmrs for device enumeration
The networkmanager plugin SHALL use the `nmrs` crate to enumerate network devices instead of manual GetDevices D-Bus calls.

#### Scenario: Enumerate ethernet devices
- **WHEN** the plugin queries for network devices
- **THEN** the system SHALL return all managed ethernet devices using nmrs device types

#### Scenario: Enumerate WiFi devices
- **WHEN** the plugin queries for network devices
- **THEN** the system SHALL return all managed WiFi devices using nmrs device types

#### Scenario: Filter virtual interfaces
- **WHEN** the plugin queries for network devices
- **THEN** the system SHALL exclude virtual interfaces (docker, veth, br-, virbr, vnet) using nmrs device properties

### Requirement: Use nmrs for device state queries
The networkmanager plugin SHALL use the `nmrs` crate to query device state and properties instead of manual D-Bus property reads.

#### Scenario: Query ethernet carrier status
- **WHEN** the plugin checks if an ethernet cable is connected
- **THEN** the system SHALL use nmrs to read the carrier property from the wired device

#### Scenario: Query device connection state
- **WHEN** the plugin checks if a device is connected
- **THEN** the system SHALL use nmrs to read the device state and active connection properties

#### Scenario: Query device management status
- **WHEN** the plugin checks if a device is managed by NetworkManager
- **THEN** the system SHALL use nmrs to read the managed and real properties

### Requirement: Use nmrs for WiFi operations
The networkmanager plugin SHALL use the `nmrs` crate for WiFi-specific operations instead of manual wireless interface D-Bus calls.

#### Scenario: Enable/disable WiFi globally
- **WHEN** the user toggles WiFi on or off
- **THEN** the system SHALL use nmrs to set the global WirelessEnabled property

#### Scenario: Request WiFi scan
- **WHEN** the user opens the WiFi menu
- **THEN** the system SHALL use nmrs to request a network scan on the wireless device

#### Scenario: Enumerate access points
- **WHEN** the plugin fetches available WiFi networks
- **THEN** the system SHALL use nmrs to get access points with their SSID, strength, and security properties

#### Scenario: Filter hidden networks
- **WHEN** the plugin enumerates access points
- **THEN** the system SHALL exclude networks with empty SSIDs using nmrs access point data

### Requirement: Use nmrs for connection management
The networkmanager plugin SHALL use the `nmrs` crate for activating and deactivating network connections where supported.

#### Scenario: Activate ethernet connection
- **WHEN** the user enables an ethernet device
- **THEN** the system SHALL use nmrs `connect_wired()` to activate the wired connection

#### Scenario: Disconnect device
- **WHEN** the user disables a network device
- **THEN** the system SHALL use nmrs `disconnect()` to disconnect the active connection

#### Scenario: Activate WiFi connection by SSID
- **WHEN** the user selects a WiFi network to connect
- **THEN** the system SHALL use D-Bus to find saved connections for that SSID and activate the connection
- **NOTE** nmrs requires credentials to connect, so D-Bus is used for saved WiFi profiles

### Requirement: Query connection details
The networkmanager plugin SHALL query connection details using the appropriate API.

**Note:** IP address, subnet, and gateway display has been removed from scope as nmrs does not expose these details directly.

#### Scenario: Query link speed
- **WHEN** the ethernet menu is expanded
- **THEN** the system SHALL use D-Bus to retrieve the link speed in Mbps from the wired device
- **NOTE** nmrs does not expose link speed, so D-Bus is used directly

### Requirement: Map nmrs types to internal store types
The networkmanager plugin SHALL convert nmrs data types to internal store types used by the application.

#### Scenario: Convert nmrs device to internal DeviceInfo
- **WHEN** the plugin receives a device from nmrs
- **THEN** the system SHALL map nmrs device properties to internal DeviceInfo structure (path, device_type, interface_name, managed, real)

#### Scenario: Convert nmrs access point to internal AccessPointState
- **WHEN** the plugin receives an access point from nmrs
- **THEN** the system SHALL map nmrs access point properties to internal AccessPointState structure (path, ssid, strength, secure, connecting)

#### Scenario: IP configuration display removed
- **NOTE** IP address, subnet, and gateway display has been removed from scope as nmrs does not expose these details directly

### Requirement: Adapter widgets call nmrs integration functions
The adapter widgets SHALL use the nmrs integration functions from dbus.rs.

#### Scenario: WiredAdapterWidget uses nmrs functions
- **WHEN** WiredAdapterWidget performs D-Bus operations
- **THEN** it SHALL call dbus::connect_wired_nmrs()
- **AND** it SHALL call dbus::disconnect_nmrs()
- **AND** it SHALL call dbus::get_link_speed()

#### Scenario: WiFiAdapterWidget uses nmrs functions
- **WHEN** WiFiAdapterWidget performs D-Bus operations
- **THEN** it SHALL call dbus::set_wifi_enabled_nmrs()
- **AND** it SHALL call dbus::scan_networks_nmrs()
- **AND** it SHALL call dbus::list_networks_nmrs()
- **AND** it SHALL call dbus::get_connections_for_ssid()
- **AND** it SHALL call dbus::activate_connection()

#### Scenario: nmrs integration functions remain unchanged
- **WHEN** the refactoring is complete
- **THEN** the nmrs integration functions in dbus.rs SHALL NOT be modified
- **AND** they SHALL be called from adapter widgets instead of mod.rs

### Requirement: Maintain backward compatibility with existing functionality
The networkmanager plugin SHALL maintain all existing functionality when migrating to nmrs.

#### Scenario: Preserve ethernet device management
- **WHEN** the migration to nmrs is complete
- **THEN** all ethernet device management features SHALL work identically to the previous implementation

#### Scenario: Preserve WiFi device management
- **WHEN** the migration to nmrs is complete
- **THEN** all WiFi device management features SHALL work identically to the previous implementation

#### Scenario: Preserve UI behavior
- **WHEN** the migration to nmrs is complete
- **THEN** the UI components SHALL receive the same store operations and display the same information as before
