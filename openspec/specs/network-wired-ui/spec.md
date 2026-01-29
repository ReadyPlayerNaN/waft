## ADDED Requirements

### Requirement: One feature toggle per wired adapter

The system SHALL create one FeatureToggleExpandableWidget per wired ethernet adapter.

#### Scenario: Single wired adapter shows one toggle

- **WHEN** the system has one wired ethernet adapter
- **THEN** one FeatureToggleExpandableWidget is created
- **AND** it is labeled "Wired ({interface_name})"

#### Scenario: Multiple wired adapters show multiple toggles

- **WHEN** the system has multiple wired ethernet adapters (e.g., built-in + USB ethernet)
- **THEN** one FeatureToggleExpandableWidget is created for each adapter
- **AND** each is labeled with its interface name

#### Scenario: Consistent with WiFi adapter pattern

- **WHEN** wired adapters are displayed
- **THEN** they follow the same pattern as WiFi adapters
- **AND** use the same FeatureToggleExpandableWidget component

### Requirement: Display correct connection status

The system SHALL display the current connection status for each wired adapter.

#### Scenario: Show connected status

- **WHEN** a wired adapter has an active connection
- **THEN** the details text shows "Connected" or connection name
- **AND** the icon shows network-wired-symbolic

#### Scenario: Show disconnected status

- **WHEN** a wired adapter is enabled but not connected (no cable)
- **THEN** the details text shows translated "Disconnected" message
- **AND** the icon shows network-wired-disconnected-symbolic

#### Scenario: Show disabled status with translation

- **WHEN** a wired adapter is disabled
- **THEN** the details text shows translated "Disabled" message (using i18n)
- **AND** NOT the hardcoded "Disabled" string

### Requirement: Wired toggle is expandable

The system SHALL allow wired adapter toggles to expand and show connection details.

#### Scenario: Toggle expands to show menu

- **WHEN** user clicks the expand chevron on a wired toggle
- **THEN** the expandable menu reveals
- **AND** connection details are displayed

#### Scenario: Toggle collapses to hide menu

- **WHEN** user clicks the expand chevron on an expanded wired toggle
- **THEN** the expandable menu collapses
- **AND** connection details are hidden

#### Scenario: Menu shows connection details when expanded

- **WHEN** a wired toggle is expanded and the connection is active
- **THEN** the menu displays connection details (link speed, IP, mask, gateway)
- **AND** uses the network-connection-details capability

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

### Requirement: Status labels are internationalized

The system SHALL use i18n translation keys for all status labels.

#### Scenario: Disabled label is translated

- **WHEN** a wired adapter is disabled
- **THEN** the status uses `i18n::t("network-disabled")`
- **AND** NOT a hardcoded English "Disabled" string

#### Scenario: Connected label is translated

- **WHEN** a wired adapter is connected
- **THEN** the status uses appropriate i18n translation key
- **AND** displays in the user's language

#### Scenario: Disconnected label is translated

- **WHEN** a wired adapter is disconnected
- **THEN** the status uses appropriate i18n translation key
- **AND** displays in the user's language

### Requirement: Wired adapter icon reflects state

The system SHALL update the wired adapter icon based on connection state.

#### Scenario: Connected icon

- **WHEN** a wired adapter is connected
- **THEN** the icon is network-wired-symbolic
- **AND** indicates active connection

#### Scenario: Disconnected icon

- **WHEN** a wired adapter is not connected (no cable or no carrier)
- **THEN** the icon is network-wired-disconnected-symbolic
- **AND** indicates no connection

#### Scenario: Disabled icon

- **WHEN** a wired adapter is disabled
- **THEN** the icon is network-wired-offline-symbolic or similar
- **AND** indicates disabled state

### Requirement: Replace custom EthernetToggleWidget

The system SHALL replace the custom EthernetToggleWidget with FeatureToggleExpandableWidget.

#### Scenario: No longer use EthernetToggleWidget

- **WHEN** wired network UI is implemented
- **THEN** EthernetToggleWidget is not used
- **AND** FeatureToggleExpandableWidget is used instead

#### Scenario: Consistent component across features

- **WHEN** viewing WiFi and wired toggles
- **THEN** both use FeatureToggleExpandableWidget
- **AND** behavior and appearance are consistent
