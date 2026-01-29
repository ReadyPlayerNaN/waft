// Integration tests for bluetooth DBus functionality require a mock BlueZ service.
//
// The bluetooth module uses complex BlueZ-specific interfaces:
// - org.bluez.Adapter1 for adapter management
// - org.bluez.Device1 for device operations
// - org.freedesktop.DBus.ObjectManager for discovering adapters and devices
//
// Unlike simpler property-based modules, bluetooth uses ObjectManager's
// GetManagedObjects which returns nested dictionaries of all objects and
// their interfaces. This makes unit testing without a real DBus service
// impractical.
//
// Future work: Add integration tests using zbus::dbus_interface macro to create
// mock BlueZ adapters and devices.
//
// Test scenarios to add:
//
// Adapter tests:
// - find_all_adapters() returns empty list when no adapters present
// - find_all_adapters() finds multiple adapters
// - find_all_adapters() correctly parses Alias/Name and Powered properties
// - find_all_adapters() sorts adapters by path
// - get_powered() returns true when adapter is powered
// - get_powered() returns false when adapter is not powered
// - get_powered() returns false as default when property missing
// - set_powered() successfully enables adapter
// - set_powered() successfully disables adapter
//
// Device tests:
// - get_paired_devices() returns empty list when no devices paired
// - get_paired_devices() finds multiple paired devices
// - get_paired_devices() filters out unpaired devices
// - get_paired_devices() only includes devices for the specified adapter
// - get_paired_devices() correctly parses device properties (Alias, Name, Icon, Connected, Paired)
// - get_paired_devices() sorts devices by name (case-insensitive)
// - connect_device() successfully connects to a device
// - disconnect_device() successfully disconnects from a device
//
// Edge cases:
// - Handles devices with missing Icon property (defaults to "bluetooth-symbolic")
// - Handles adapters with missing Alias (falls back to Name)
// - Handles devices with missing Name/Alias (falls back to i18n string)
