## ADDED Requirements

### Requirement: Typed Property Getters

The system SHALL provide generic property getter methods on `DbusHandle` that return typed values with default fallbacks.

#### Scenario: Get typed property with default

- **WHEN** `get_typed_property::<T>()` is called with a property that exists
- **THEN** it returns `Ok(T)` with the typed value

#### Scenario: Get typed property returns default on missing

- **WHEN** `get_typed_property::<T>()` is called with a missing property
- **THEN** it returns `Ok(T::default())`

#### Scenario: Get typed property fails on type mismatch

- **WHEN** `get_typed_property::<T>()` is called on a property with incompatible type
- **THEN** it returns an `Err` result with type conversion error

#### Scenario: Supported types include common DBus primitives

- **WHEN** typed property getter is used
- **THEN** it supports at minimum: `String`, `bool`, `u32`, `u64`, `i32`, `i64`, `f64`

### Requirement: GetAll Property Fetcher

The system SHALL provide a wrapper for `org.freedesktop.DBus.Properties.GetAll` that returns a typed HashMap.

#### Scenario: Get all properties succeeds

- **WHEN** `get_all_properties()` is called with destination, path, and interface
- **THEN** it returns `Ok(HashMap<String, OwnedValue>)` with all properties
- **AND** the HashMap can be queried for individual properties

#### Scenario: Extract typed values from GetAll result

- **WHEN** GetAll result is obtained
- **THEN** individual properties can be extracted using value conversion helpers
- **AND** missing properties return sensible defaults

### Requirement: PropertiesChanged Signal Listener

The system SHALL provide a helper for listening to `org.freedesktop.DBus.Properties.PropertiesChanged` signals.

#### Scenario: Listen for property changes with callback

- **WHEN** `listen_properties_changed()` is called with destination, path, interface, and callback
- **THEN** PropertiesChanged signals trigger the callback
- **AND** the callback receives the interface name and changed properties HashMap
- **AND** signals from other interfaces are filtered out

#### Scenario: Property change listener handles multiple properties

- **WHEN** a PropertiesChanged signal contains multiple changed properties
- **THEN** all properties are included in the callback HashMap

#### Scenario: Property change listener runs in background

- **WHEN** `listen_properties_changed()` returns
- **THEN** the listener task runs in the background
- **AND** the main task can continue without blocking

### Requirement: Consolidated Value Extractors

The system SHALL provide value extraction functions in `src/dbus.rs` that replace duplicated implementations in feature modules.

#### Scenario: owned_value_to_bool extracts boolean

- **WHEN** `owned_value_to_bool()` receives an `OwnedValue` containing a boolean
- **THEN** it returns `Some(bool)` with the value

#### Scenario: owned_value_to_bool returns None for non-boolean

- **WHEN** `owned_value_to_bool()` receives a non-boolean type
- **THEN** it returns `None`

#### Scenario: owned_value_to_u32 extracts u32

- **WHEN** `owned_value_to_u32()` receives an `OwnedValue` containing a u32
- **THEN** it returns `Some(u32)` with the value

#### Scenario: owned_value_to_u32 returns None for non-u32

- **WHEN** `owned_value_to_u32()` receives a non-u32 type
- **THEN** it returns `None`

#### Scenario: Value extractors handle OwnedValue cloning

- **WHEN** value extractors are called with cloned `OwnedValue`
- **THEN** they work correctly without ownership issues

### Requirement: NetworkManager Standardization

Feature modules SHALL use `DbusHandle` consistently instead of creating raw `Connection` instances.

#### Scenario: NetworkManager uses DbusHandle

- **WHEN** networkmanager module functions are called
- **THEN** they accept `&DbusHandle` parameter
- **AND** they do not create raw `Connection::system()` instances

#### Scenario: Backward compatibility maintained

- **WHEN** networkmanager module is refactored to use DbusHandle
- **THEN** all existing functionality continues to work
- **AND** the public API remains unchanged
