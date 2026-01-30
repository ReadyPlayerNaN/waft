## ADDED Requirements

### Requirement: VPN toggle displays connection status
The VPN toggle widget SHALL display "VPN" as the title when no VPN is connected, or the connected VPN's name when a VPN is active.

#### Scenario: No VPN connected
- **WHEN** no VPN connection is active
- **THEN** the toggle displays "VPN" as its title

#### Scenario: VPN connected
- **WHEN** a VPN named "Work VPN" is connected
- **THEN** the toggle displays "Work VPN" as its title

### Requirement: VPN toggle shows connection state details
The VPN toggle widget SHALL display the current connection state as details text below the title.

#### Scenario: Disconnected state
- **WHEN** no VPN is connected
- **THEN** the toggle displays "Disconnected" as details

#### Scenario: Connected state
- **WHEN** a VPN is connected
- **THEN** the toggle displays "Connected" as details

#### Scenario: Connecting state
- **WHEN** a VPN is in the process of connecting
- **THEN** the toggle displays "Connecting..." as details

#### Scenario: Disconnecting state
- **WHEN** a VPN is in the process of disconnecting
- **THEN** the toggle displays "Disconnecting..." as details

### Requirement: VPN toggle click behavior depends on state
The VPN toggle widget SHALL disconnect the active VPN when clicked while connected, and expand the menu when clicked while disconnected.

#### Scenario: Click while connected
- **WHEN** user clicks the toggle while a VPN is connected
- **THEN** the system initiates disconnection of the active VPN

#### Scenario: Click while disconnected
- **WHEN** user clicks the toggle while no VPN is connected
- **THEN** the toggle expands to show the VPN menu

### Requirement: VPN menu lists configured VPNs
The VPN menu widget SHALL display all configured VPN connections with their current state.

#### Scenario: Multiple VPNs configured
- **WHEN** user has configured VPNs named "Work VPN", "Home VPN", and "Travel VPN"
- **THEN** the menu displays three rows, one for each VPN

#### Scenario: No VPNs configured
- **WHEN** user has no configured VPN connections
- **THEN** the VPN toggle is not displayed (widget is not created)

### Requirement: VPN menu row structure
Each VPN row in the menu SHALL display a VPN icon, the VPN name, a spinner (when connecting/disconnecting), and a switch toggle.

#### Scenario: Row layout
- **WHEN** the menu displays a VPN named "Work VPN"
- **THEN** the row shows: VPN icon (left), "Work VPN" label (center, expanding), switch (right)

#### Scenario: Connecting state shows spinner
- **WHEN** "Work VPN" is in connecting state
- **THEN** the row displays a spinning indicator and the switch is disabled

#### Scenario: Disconnecting state shows spinner
- **WHEN** "Work VPN" is in disconnecting state
- **THEN** the row displays a spinning indicator and the switch is disabled

### Requirement: VPN menu row is fully clickable
The entire VPN menu row SHALL be clickable and trigger the same action as toggling the switch.

#### Scenario: Click row to connect
- **WHEN** user clicks anywhere on a disconnected VPN row
- **THEN** the system initiates connection to that VPN

#### Scenario: Click row to disconnect
- **WHEN** user clicks anywhere on a connected VPN row
- **THEN** the system initiates disconnection of that VPN

### Requirement: VPN switch reflects connection state
The switch in each VPN row SHALL reflect the current connection state of that VPN.

#### Scenario: Connected VPN switch
- **WHEN** "Work VPN" is connected
- **THEN** its switch is in the ON position

#### Scenario: Disconnected VPN switch
- **WHEN** "Work VPN" is disconnected
- **THEN** its switch is in the OFF position

### Requirement: VPN state updates in real-time
The VPN widget SHALL update its display when VPN connection states change.

#### Scenario: VPN connects
- **WHEN** "Work VPN" transitions from Disconnected to Connected
- **THEN** the toggle updates to show "Work VPN" as title and "Connected" as details
- **AND** the menu row switch updates to ON position

#### Scenario: VPN disconnects
- **WHEN** "Work VPN" transitions from Connected to Disconnected
- **THEN** the toggle updates to show "VPN" as title and "Disconnected" as details
- **AND** the menu row switch updates to OFF position

### Requirement: VPN activation uses saved credentials
The system SHALL activate VPN connections using saved connection profiles without prompting for credentials.

#### Scenario: Activate saved VPN
- **WHEN** user initiates connection to a configured VPN
- **THEN** the system activates the VPN using NetworkManager's saved connection profile

### Requirement: VPN widget registration
The VPN widget SHALL be registered as a feature toggle with a unique identifier and appropriate weight.

#### Scenario: Widget registration
- **WHEN** the networkmanager plugin initializes
- **THEN** a VPN feature toggle is registered with ID "networkmanager:vpn"

### Requirement: VPN translations
All VPN-related labels SHALL use translated strings.

#### Scenario: VPN title translation
- **WHEN** displaying the VPN toggle title with no connection
- **THEN** the system uses the translated string for "VPN"

#### Scenario: VPN state translations
- **WHEN** displaying connection states
- **THEN** the system uses translated strings for "Connected", "Disconnected", "Connecting...", "Disconnecting..."
