## ADDED Requirements

### Requirement: D-Bus client connects to keyboard layout service
The system SHALL establish a compositor-agnostic D-Bus connection for keyboard layout management.

#### Scenario: Client connects to systemd-localed
- **WHEN** the KeyboardLayoutClient is initialized
- **THEN** it SHALL attempt to connect to `org.freedesktop.locale1` on the system bus
- **AND** it SHALL use the object path `/org/freedesktop/locale1`
- **AND** it SHALL use the interface `org.freedesktop.locale1`

#### Scenario: Client falls back to alternative services
- **WHEN** systemd-localed is unavailable
- **THEN** the client SHALL attempt to connect to alternative XKB-aware services
- **AND** it SHALL try compositor-specific D-Bus interfaces if available
- **AND** it SHALL return an error if no suitable service is found

#### Scenario: Connection failure is handled gracefully
- **WHEN** D-Bus connection fails during initialization
- **THEN** the client SHALL return a Result::Err with error details
- **AND** it SHALL NOT panic or crash the application
- **AND** the calling code SHALL handle the unavailability appropriately

### Requirement: Client queries current keyboard layout
The system SHALL retrieve the active keyboard layout via D-Bus properties.

#### Scenario: Query current layout from locale1
- **WHEN** `get_current_layout()` is called
- **THEN** the client SHALL read the `X11Layout` property from org.freedesktop.locale1
- **AND** it SHALL parse the layout string (e.g., "us", "de", "fr")
- **AND** it SHALL return the layout as an uppercase abbreviation

#### Scenario: Multiple layouts return the active one
- **WHEN** multiple layouts are configured (e.g., "us,de,fr")
- **THEN** the client SHALL parse the comma-separated list
- **AND** it SHALL identify the currently active layout from XKB state
- **AND** it SHALL return only the active layout abbreviation

#### Scenario: Layout query failure returns error
- **WHEN** D-Bus property read fails
- **THEN** `get_current_layout()` SHALL return Result::Err
- **AND** the error SHALL include the D-Bus error details
- **AND** the caller SHALL decide on fallback behavior

### Requirement: Client queries available keyboard layouts
The system SHALL retrieve the list of all configured keyboard layouts.

#### Scenario: Query available layouts from locale1
- **WHEN** `get_available_layouts()` is called
- **THEN** the client SHALL read the `X11Layout` property
- **AND** it SHALL parse the comma-separated layout list (e.g., "us,de,fr")
- **AND** it SHALL return a Vec of layout abbreviations in configured order

#### Scenario: Single layout returns single-item list
- **WHEN** only one layout is configured
- **THEN** `get_available_layouts()` SHALL return a Vec with one element
- **AND** cycling SHALL be effectively a no-op

#### Scenario: No layouts returns empty list
- **WHEN** no layouts are configured (edge case)
- **THEN** `get_available_layouts()` SHALL return an empty Vec
- **AND** the caller SHALL handle the empty case appropriately

### Requirement: Client switches keyboard layout
The system SHALL change the active keyboard layout via D-Bus method calls.

#### Scenario: Switch to specific layout
- **WHEN** `set_layout(layout_abbr)` is called
- **THEN** the client SHALL invoke the `SetX11Keyboard` method on org.freedesktop.locale1
- **AND** it SHALL pass the layout abbreviation as the first argument
- **AND** it SHALL preserve other XKB settings (model, variant, options)

#### Scenario: Switch to next layout in sequence
- **WHEN** `cycle_layout()` is called
- **THEN** the client SHALL query the current layout
- **AND** it SHALL query the available layouts list
- **AND** it SHALL determine the next layout in the sequence
- **AND** it SHALL call `set_layout()` with the next layout

#### Scenario: Cycle wraps around at end of list
- **WHEN** `cycle_layout()` is called on the last layout
- **THEN** the client SHALL wrap to the first layout in the available list
- **AND** it SHALL switch to that first layout

#### Scenario: Layout switch failure returns error
- **WHEN** D-Bus method call fails
- **THEN** `set_layout()` SHALL return Result::Err with error details
- **AND** the active layout SHALL remain unchanged
- **AND** the caller SHALL handle the error appropriately

### Requirement: Client subscribes to layout change signals
The system SHALL notify subscribers when the keyboard layout changes externally.

#### Scenario: Subscribe to PropertiesChanged signal
- **WHEN** `subscribe_layout_changes(callback)` is called
- **THEN** the client SHALL subscribe to `PropertiesChanged` signals
- **AND** it SHALL filter for changes to the `X11Layout` property
- **AND** it SHALL invoke the callback with the new layout when it changes

#### Scenario: External layout change triggers callback
- **WHEN** an external tool changes the keyboard layout via D-Bus
- **THEN** the client SHALL receive a PropertiesChanged signal
- **AND** it SHALL parse the new layout from the signal data
- **AND** it SHALL invoke all registered callbacks with the new layout abbreviation

#### Scenario: Multiple subscribers receive notifications
- **WHEN** multiple components subscribe to layout changes
- **THEN** all callbacks SHALL be invoked when a change occurs
- **AND** callbacks SHALL be invoked in registration order
- **AND** callback failures SHALL NOT prevent other callbacks from executing

#### Scenario: Unsubscribe stops notifications
- **WHEN** a subscriber calls `unsubscribe_layout_changes()`
- **THEN** its callback SHALL be removed from the notification list
- **AND** it SHALL NOT receive further layout change notifications

### Requirement: Client parses XKB layout format
The system SHALL correctly parse XKB layout strings into usable abbreviations.

#### Scenario: Parse simple layout string
- **WHEN** parsing "us"
- **THEN** the client SHALL return "US"

#### Scenario: Parse layout with variant
- **WHEN** parsing "us(dvorak)"
- **THEN** the client SHALL return "US" (variant information ignored for display)

#### Scenario: Parse multi-layout string
- **WHEN** parsing "us,de,fr"
- **THEN** the client SHALL return ["US", "DE", "FR"]

#### Scenario: Parse invalid layout string
- **WHEN** parsing an empty string or malformed input
- **THEN** the client SHALL return a default value or error
- **AND** it SHALL log the parsing failure

### Requirement: Client handles D-Bus errors
The system SHALL provide clear error handling for all D-Bus operations.

#### Scenario: Connection lost during operation
- **WHEN** D-Bus connection is lost during a method call
- **THEN** the client SHALL return Result::Err with connection error
- **AND** it SHALL attempt to reconnect on the next operation
- **AND** subscribers SHALL be notified of connection state changes

#### Scenario: Service becomes unavailable
- **WHEN** org.freedesktop.locale1 service stops responding
- **THEN** D-Bus operations SHALL timeout with clear error messages
- **AND** the client SHALL indicate service unavailability to callers

#### Scenario: Permission denied error
- **WHEN** D-Bus method call fails due to PolicyKit/permissions
- **THEN** the client SHALL return Result::Err indicating permission denial
- **AND** the error message SHALL suggest authentication requirements

### Requirement: Client is thread-safe
The system SHALL allow concurrent access from multiple threads or async contexts.

#### Scenario: Concurrent layout queries
- **WHEN** multiple threads call `get_current_layout()` simultaneously
- **THEN** all calls SHALL complete successfully
- **AND** all SHALL return the same layout value
- **AND** no race conditions or deadlocks SHALL occur

#### Scenario: Async-safe signal subscription
- **WHEN** layout change signals are received in async context
- **THEN** callbacks SHALL be invoked in a thread-safe manner
- **AND** callbacks SHALL not block the D-Bus event loop
- **AND** long-running callbacks SHALL be executed via async spawn or similar

### Requirement: Client provides testable interface
The system SHALL support mocking and testing of D-Bus interactions.

#### Scenario: Mock client for unit tests
- **WHEN** unit tests need to verify layout switching logic
- **THEN** a MockKeyboardLayoutClient SHALL be available
- **AND** it SHALL implement the same trait as the real client
- **AND** it SHALL allow tests to control return values and simulate errors

#### Scenario: Integration tests use real D-Bus
- **WHEN** integration tests run with D-Bus available
- **THEN** the real KeyboardLayoutClient SHALL be used
- **AND** tests SHALL verify actual D-Bus communication
- **AND** tests SHALL clean up any layout changes made during testing
