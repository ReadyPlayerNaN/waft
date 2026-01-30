### Requirement: Retrieve IPv4 configuration from NetworkManager

The system SHALL provide a function to retrieve IPv4 configuration for a network device.

#### Scenario: IPv4 config retrieved for connected device

- **WHEN** requesting IP configuration for a connected wired device
- **THEN** the function SHALL return the IPv4 address, prefix length, and gateway
- **AND** the values SHALL be retrieved from the device's IP4Config D-Bus object

#### Scenario: IPv4 config path is empty

- **WHEN** the device's Ip4Config property is "/" (no config)
- **THEN** the function SHALL return None for IPv4 configuration
- **AND** no error SHALL be raised

#### Scenario: Multiple IPv4 addresses assigned

- **WHEN** the device has multiple IPv4 addresses
- **THEN** the function SHALL return the first address from AddressData
- **AND** additional addresses SHALL be ignored

### Requirement: Retrieve IPv6 configuration from NetworkManager

The system SHALL provide a function to retrieve IPv6 configuration for a network device.

#### Scenario: IPv6 config retrieved for connected device

- **WHEN** requesting IP configuration for a connected wired device with IPv6
- **THEN** the function SHALL return the IPv6 address and prefix length
- **AND** the values SHALL be retrieved from the device's IP6Config D-Bus object

#### Scenario: IPv6 config path is empty

- **WHEN** the device's Ip6Config property is "/" (no config)
- **THEN** the function SHALL return None for IPv6 configuration
- **AND** no error SHALL be raised

### Requirement: Convert prefix length to subnet mask

The system SHALL convert CIDR prefix length to dotted decimal subnet mask for display.

#### Scenario: Common prefix lengths converted correctly

- **WHEN** prefix length is 24
- **THEN** subnet mask SHALL be "255.255.255.0"

#### Scenario: Prefix length 16

- **WHEN** prefix length is 16
- **THEN** subnet mask SHALL be "255.255.0.0"

#### Scenario: Prefix length 8

- **WHEN** prefix length is 8
- **THEN** subnet mask SHALL be "255.0.0.0"

### Requirement: IP configuration struct contains all fields

The system SHALL return IP configuration as a structured type with all relevant fields.

#### Scenario: IpConfiguration contains IPv4 data

- **WHEN** IPv4 configuration is available
- **THEN** the struct SHALL contain ipv4_address, subnet_mask, and gateway fields
- **AND** each field SHALL be Option<String> to handle missing values

#### Scenario: IpConfiguration contains IPv6 data

- **WHEN** IPv6 configuration is available
- **THEN** the struct SHALL contain ipv6_address field
- **AND** the field SHALL be Option<String>
