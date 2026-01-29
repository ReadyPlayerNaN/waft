## ADDED Requirements

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
