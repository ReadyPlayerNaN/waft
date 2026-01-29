## ADDED Requirements

### Requirement: Display link speed

The system SHALL display the network link speed when a wired connection is active.

#### Scenario: Link speed shown for active connection

- **WHEN** a wired ethernet connection is active
- **THEN** the link speed is displayed (e.g., "1 Gbps", "100 Mbps")
- **AND** the value is retrieved from NetworkManager device properties

#### Scenario: Link speed not shown when disconnected

- **WHEN** a wired ethernet connection is not active
- **THEN** the link speed field is not displayed
- **AND** no placeholder or default value is shown

### Requirement: Display IPv4 address

The system SHALL display the IPv4 address when assigned to a wired connection.

#### Scenario: IPv4 address shown for connected device

- **WHEN** a wired ethernet connection has an IPv4 address assigned
- **THEN** the IPv4 address is displayed (e.g., "192.168.1.100")
- **AND** the value is retrieved from NetworkManager IP4Config

#### Scenario: IPv4 address not shown when unavailable

- **WHEN** a wired ethernet connection has no IPv4 address
- **THEN** the IPv4 address field is not displayed
- **AND** no error or placeholder is shown

### Requirement: Display IPv6 address

The system SHALL display the IPv6 address when assigned to a wired connection.

#### Scenario: IPv6 address shown for connected device

- **WHEN** a wired ethernet connection has an IPv6 address assigned
- **THEN** the IPv6 address is displayed
- **AND** the value is retrieved from NetworkManager IP6Config

#### Scenario: IPv6 address not shown when unavailable

- **WHEN** a wired ethernet connection has no IPv6 address
- **THEN** the IPv6 address field is not displayed
- **AND** no error or placeholder is shown

### Requirement: Display subnet mask

The system SHALL display the subnet mask when a wired connection is active.

#### Scenario: Subnet mask shown for active connection

- **WHEN** a wired ethernet connection has an IPv4 address
- **THEN** the subnet mask is displayed (e.g., "255.255.255.0" or "/24" notation)
- **AND** the value is retrieved from NetworkManager IP4Config

### Requirement: Display default gateway

The system SHALL display the default gateway address when configured.

#### Scenario: Gateway shown for active connection

- **WHEN** a wired ethernet connection has a default gateway configured
- **THEN** the gateway address is displayed (e.g., "192.168.1.1")
- **AND** the value is retrieved from NetworkManager IP4Config or IP6Config

#### Scenario: Gateway not shown when unconfigured

- **WHEN** a wired ethernet connection has no default gateway
- **THEN** the gateway field is not displayed
- **AND** no placeholder is shown

### Requirement: Connection details layout

The system SHALL display connection details as a vertical list of label-value pairs.

#### Scenario: Label-value pair format

- **WHEN** connection details are displayed
- **THEN** each detail is shown as a label-value pair (e.g., "IP Address: 192.168.1.100")
- **AND** the layout is vertical with consistent spacing

#### Scenario: Empty state when disconnected

- **WHEN** no wired connection is active
- **THEN** connection details section is empty or shows "Not connected" message
- **AND** no partial or stale information is displayed

### Requirement: Connection details refresh on change

The system SHALL update connection details when the connection state changes.

#### Scenario: Details update on connection

- **WHEN** a wired connection becomes active
- **THEN** connection details are fetched and displayed
- **AND** the UI updates without requiring manual refresh

#### Scenario: Details clear on disconnection

- **WHEN** a wired connection is disconnected
- **THEN** connection details are cleared from display
- **AND** the UI updates immediately
