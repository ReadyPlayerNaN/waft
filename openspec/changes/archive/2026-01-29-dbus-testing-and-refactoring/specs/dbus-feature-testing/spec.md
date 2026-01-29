## ADDED Requirements

### Requirement: Darkman Module Tests

The test suite SHALL verify darkman DBus integration functions.

#### Scenario: Get darkman state succeeds

- **WHEN** `get_state()` is called with a valid DBus handle
- **THEN** it returns `Ok(DarkmanMode)` with the current mode
- **AND** the mode is either Light or Dark

#### Scenario: Set darkman state succeeds

- **WHEN** `set_state()` is called with a valid mode
- **THEN** it returns `Ok(())`
- **AND** subsequent `get_state()` returns the new mode

### Requirement: Battery Module Tests

The test suite SHALL verify UPower DBus integration functions.

#### Scenario: Get battery info succeeds

- **WHEN** `get_battery_info()` is called
- **THEN** it returns `Ok(BatteryInfo)` with all properties populated
- **AND** percentage is between 0.0 and 100.0
- **AND** state is a valid BatteryState variant

#### Scenario: Listen battery changes receives updates

- **WHEN** `listen_battery_changes()` is called with a channel
- **THEN** PropertiesChanged signals trigger info updates
- **AND** updated info is sent through the channel
- **AND** only Device interface changes trigger updates

### Requirement: Bluetooth Module Tests

The test suite SHALL verify BlueZ DBus integration functions.

#### Scenario: Find all adapters succeeds

- **WHEN** `find_all_adapters()` is called
- **THEN** it returns `Ok(Vec<BluetoothAdapter>)` with all adapters
- **AND** adapters are sorted by path

#### Scenario: Get adapter powered state succeeds

- **WHEN** `get_powered()` is called with an adapter path
- **THEN** it returns `Ok(bool)` with the powered state

#### Scenario: Set adapter powered state succeeds

- **WHEN** `set_powered()` is called with an adapter path and state
- **THEN** it returns `Ok(())`
- **AND** subsequent `get_powered()` returns the new state

#### Scenario: Find paired devices succeeds

- **WHEN** `find_paired_devices()` is called with an adapter path
- **THEN** it returns `Ok(Vec<BluetoothDevice>)` with paired devices
- **AND** each device has name, icon, paired, and connected status

### Requirement: Audio Module Tests

The test suite SHALL verify PulseAudio/PipeWire pactl integration.

#### Scenario: Get card port info succeeds

- **WHEN** `get_card_port_info()` is called
- **THEN** it returns `Ok(CardPortMap)` with parsed card ports

#### Scenario: Get sinks succeeds

- **WHEN** `get_sinks()` is called
- **THEN** it returns `Ok(Vec<SinkInfo>)` with all output devices
- **AND** each sink has name, description, volume, and mute status

#### Scenario: Get sources succeeds

- **WHEN** `get_sources()` is called
- **THEN** it returns `Ok(Vec<SourceInfo>)` with all input devices

#### Scenario: Set default sink succeeds

- **WHEN** `set_default_sink()` is called with a sink name
- **THEN** it returns `Ok(())`

#### Scenario: Set sink volume succeeds

- **WHEN** `set_sink_volume()` is called with sink name and percentage
- **THEN** it returns `Ok(())`

### Requirement: NetworkManager Module Tests

The test suite SHALL verify NetworkManager DBus integration functions.

#### Scenario: Check availability succeeds

- **WHEN** `check_availability()` is called
- **THEN** it returns `true` if NetworkManager is running
- **AND** returns `false` if NetworkManager is not available

#### Scenario: Get all devices succeeds

- **WHEN** `get_all_devices()` is called
- **THEN** it returns `Ok(Vec<DeviceInfo>)` with ethernet and wifi devices
- **AND** other device types are filtered out

#### Scenario: Get device property succeeds

- **WHEN** `get_device_property()` is called with device path and property name
- **THEN** it returns the typed property value

### Requirement: Agenda Module Tests

The test suite SHALL verify calendar DBus integration functions.

#### Scenario: Get events succeeds

- **WHEN** agenda module queries calendar events
- **THEN** it returns structured event data
- **AND** events include title, time, and location
