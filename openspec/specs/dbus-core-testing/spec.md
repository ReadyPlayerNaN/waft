## ADDED Requirements

### Requirement: DbusHandle Connection Tests

The test suite SHALL verify that `DbusHandle` can establish connections to both session and system buses.

#### Scenario: Session bus connection succeeds

- **WHEN** `DbusHandle::connect()` is called
- **THEN** it returns a valid `DbusHandle` instance
- **AND** the handle can be cloned

#### Scenario: System bus connection succeeds

- **WHEN** `DbusHandle::connect_system()` is called
- **THEN** it returns a valid `DbusHandle` instance
- **AND** the handle can access the underlying connection

### Requirement: Property Get/Set Operations

The test suite SHALL verify that `DbusHandle` can read and write DBus properties via the `org.freedesktop.DBus.Properties` interface.

#### Scenario: Get string property succeeds

- **WHEN** `get_property()` is called with valid destination, path, and property name
- **THEN** it returns `Ok(Some(String))` with the property value
- **AND** the string matches the expected value

#### Scenario: Get non-string property returns None

- **WHEN** `get_property()` is called on a non-string property
- **THEN** it returns `Ok(None)`

#### Scenario: Get non-existent property fails

- **WHEN** `get_property()` is called with an invalid property name
- **THEN** it returns an `Err` result

#### Scenario: Set property succeeds

- **WHEN** `set_property()` is called with valid destination, path, property name, and value
- **THEN** it returns `Ok(())`
- **AND** subsequent `get_property()` returns the new value

### Requirement: Signal Listening

The test suite SHALL verify that `DbusHandle` can listen for DBus signals using match rules.

#### Scenario: Signal listener receives matching signals

- **WHEN** `listen_signals()` is called with a valid match rule
- **THEN** it returns a broadcast receiver
- **AND** the receiver receives signals matching the rule
- **AND** signals not matching the rule are filtered out

#### Scenario: Signal listener handles interface filtering

- **WHEN** match rule specifies an interface
- **THEN** only signals from that interface are received

#### Scenario: Signal listener handles member filtering

- **WHEN** match rule specifies a member
- **THEN** only signals with that member name are received

#### Scenario: Value listener extracts string values

- **WHEN** `listen_for_values()` is called with interface and member
- **THEN** the callback receives `Some(String)` for signals with string payload
- **AND** the callback receives `None` for signals without valid string payload

### Requirement: Value Conversion Helpers

The test suite SHALL verify that value conversion functions correctly extract typed values from `OwnedValue`.

#### Scenario: owned_value_to_string extracts string

- **WHEN** `owned_value_to_string()` receives an `OwnedValue` containing a string
- **THEN** it returns `Some(String)` with the extracted value

#### Scenario: owned_value_to_string returns None for non-string

- **WHEN** `owned_value_to_string()` receives an `OwnedValue` containing a non-string type
- **THEN** it returns `None`

#### Scenario: decode_first_body_string extracts from message

- **WHEN** `decode_first_body_string()` receives a message with string body
- **THEN** it returns `Some(String)` with the first body field

#### Scenario: escape_match_value escapes quotes

- **WHEN** `escape_match_value()` receives a string containing single quotes
- **THEN** it returns a string with quotes escaped for DBus match rules
