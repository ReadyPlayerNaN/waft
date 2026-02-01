## ADDED Requirements

### Requirement: Keyboard layout button appears in header
The system SHALL display a keyboard layout indicator button in the main overlay header slot.

#### Scenario: Layout button is registered
- **WHEN** the keyboard_layout plugin creates elements
- **THEN** it SHALL register a Widget with slot `Slot::Header`
- **AND** the widget ID SHALL be `keyboard-layout:indicator`
- **AND** the widget weight SHALL be 95 or higher to position rightward
- **AND** the widget SHALL display the current keyboard layout abbreviation (e.g., "US", "DE", "FR")

#### Scenario: Layout button displays current layout
- **WHEN** the keyboard layout widget is created
- **THEN** it SHALL query the current keyboard layout via D-Bus
- **AND** it SHALL display the layout abbreviation as the button label
- **AND** the label SHALL be in uppercase (e.g., "US" not "us")

### Requirement: Layout button is clickable
The system SHALL cycle through available keyboard layouts when the button is clicked.

#### Scenario: Click button to cycle layouts
- **WHEN** the user clicks the keyboard layout button
- **THEN** the system SHALL determine the next layout in the configured sequence
- **AND** it SHALL emit a layout switch request via D-Bus
- **AND** it SHALL update the button label to reflect the new layout

#### Scenario: Cycle wraps around to first layout
- **WHEN** the user clicks the button while on the last available layout
- **THEN** the system SHALL cycle to the first layout in the sequence
- **AND** the button label SHALL update to show the first layout

### Requirement: Layout updates are reflected in UI
The system SHALL update the button label when the keyboard layout changes externally.

#### Scenario: External layout change updates button
- **WHEN** the keyboard layout is changed by an external tool or compositor
- **THEN** the widget SHALL receive a D-Bus signal notification
- **AND** it SHALL update the button label to reflect the new layout
- **AND** the update SHALL occur without user interaction

#### Scenario: Initial layout is displayed on startup
- **WHEN** the widget is first created
- **THEN** it SHALL query the current layout from D-Bus
- **AND** it SHALL display the layout immediately upon initialization
- **AND** it SHALL handle D-Bus unavailability gracefully with a fallback label

### Requirement: Widget uses consistent styling
The system SHALL apply CSS classes for consistent visual styling.

#### Scenario: Layout button has CSS classes
- **WHEN** the keyboard layout widget is created
- **THEN** the root button SHALL have CSS class `keyboard-layout-button`
- **AND** the label SHALL have CSS class `keyboard-layout-label`

#### Scenario: Active state uses CSS class
- **WHEN** the user clicks the keyboard layout button
- **THEN** the button SHALL add CSS class `active` during the transition
- **AND** CSS transitions SHALL animate the layout change
- **AND** the `active` class SHALL be removed after the transition completes

### Requirement: Widget lifecycle follows plugin pattern
The system SHALL manage widget lifecycle through plugin registration and cleanup.

#### Scenario: Widget is registered during plugin initialization
- **WHEN** `Plugin::create_elements()` is called on KeyboardLayoutPlugin
- **THEN** it SHALL create a KeyboardLayoutWidget instance
- **AND** it SHALL register the widget via WidgetRegistrar
- **AND** the widget SHALL subscribe to D-Bus layout change signals

#### Scenario: Widget handles D-Bus unavailability gracefully
- **WHEN** the D-Bus client fails to initialize
- **THEN** the plugin SHALL still register the widget with a fallback label (e.g., "??")
- **AND** the widget SHALL display a visual indication of unavailability
- **AND** the plugin SHALL NOT crash or block initialization

#### Scenario: Widget cleans up on destruction
- **WHEN** the KeyboardLayoutWidget is destroyed
- **THEN** it SHALL unsubscribe from D-Bus signals
- **AND** it SHALL release all D-Bus resources
- **AND** it SHALL not receive further state updates

### Requirement: Widget provides accessibility support
The system SHALL provide accessible labels and keyboard navigation.

#### Scenario: Button has accessible label
- **WHEN** the keyboard layout widget is created
- **THEN** the button SHALL have an accessible name "Keyboard Layout"
- **AND** the accessible description SHALL include the current layout (e.g., "Current layout: US")

#### Scenario: Button is keyboard navigable
- **WHEN** the user navigates to the layout button using keyboard
- **THEN** the button SHALL be focusable via Tab key
- **AND** pressing Enter or Space SHALL cycle the layout
- **AND** focus indication SHALL be visible via CSS

### Requirement: Widget handles errors gracefully
The system SHALL display appropriate feedback when layout switching fails.

#### Scenario: Layout switch failure shows indication
- **WHEN** a D-Bus layout switch request fails
- **THEN** the widget SHALL revert to the previous layout label
- **AND** it SHALL log the error for debugging
- **AND** it SHALL NOT display an error dialog to the user

#### Scenario: Empty layout list shows fallback
- **WHEN** D-Bus reports zero available layouts
- **THEN** the widget SHALL display a fallback label (e.g., "N/A")
- **AND** the button SHALL be non-clickable
- **AND** it SHALL retry querying layouts on the next D-Bus reconnection
