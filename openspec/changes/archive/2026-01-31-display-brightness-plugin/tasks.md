## 1. Module Setup

- [x] 1.1 Create `src/features/brightness/` directory structure with mod.rs, store.rs, dbus.rs
- [x] 1.2 Add brightness module export to `src/features/mod.rs`
- [x] 1.3 Register BrightnessPlugin in the plugin list

## 2. Backend Discovery (dbus.rs)

- [x] 2.1 Implement `is_brightnessctl_available()` to check if brightnessctl CLI exists
- [x] 2.2 Implement `is_ddcutil_available()` to check if ddcutil CLI exists
- [x] 2.3 Implement `discover_backlight_devices()` using `brightnessctl -l -m` to enumerate devices
- [x] 2.4 Implement `discover_ddc_monitors()` using `ddcutil detect` to enumerate DDC/CI monitors
- [x] 2.5 Implement `get_brightness(device)` to read current brightness (0.0-1.0) for a device
- [x] 2.6 Implement `set_brightness(device, value)` to set brightness for a device

## 3. State Management (store.rs)

- [x] 3.1 Define `Display` struct with id, name, display_type (Backlight/External), brightness
- [x] 3.2 Define `BrightnessState` struct with displays vec and available flag
- [x] 3.3 Define `BrightnessOp` enum for store operations (SetDisplays, SetBrightness, SetAvailable)
- [x] 3.4 Implement `create_brightness_store()` with reducer logic
- [x] 3.5 Implement `compute_master_average()` helper function

## 4. Display Menu Widget

- [x] 4.1 Create `display_menu.rs` with `DisplayMenuWidget` struct
- [x] 4.2 Implement menu row: icon + slider + truncated label layout
- [x] 4.3 Implement `set_displays()` to populate menu with per-display sliders
- [x] 4.4 Implement output callback for individual slider changes
- [x] 4.5 Implement `update_brightness(display_id, value)` to update individual slider

## 5. Brightness Control Widget

- [x] 5.1 Create `control_widget.rs` with `BrightnessControlWidget` struct
- [x] 5.2 Create master SliderControlWidget with `display-brightness-symbolic` icon
- [x] 5.3 Conditionally attach DisplayMenuWidget when 2+ displays exist
- [x] 5.4 Implement master slider value calculation (average of all displays)
- [x] 5.5 Implement proportional scaling logic for master slider changes
- [x] 5.6 Implement special case: additive scaling when all displays at 0%
- [x] 5.7 Connect individual slider changes to update master average

## 6. Plugin Integration (mod.rs)

- [x] 6.1 Implement `BrightnessPlugin` struct with store and widget references
- [x] 6.2 Implement `init()`: check backend availability, discover displays, load initial brightness
- [x] 6.3 Implement `create_elements()`: create widget, register in Controls slot with weight 60
- [x] 6.4 Connect widget output events to backend calls via store
- [x] 6.5 Handle graceful degradation: skip registration if no displays found

## 7. Testing

- [x] 7.1 Add unit tests for `compute_master_average()` function
- [x] 7.2 Add unit tests for proportional scaling formula including edge cases
- [x] 7.3 Add dbus_tests.rs with mock tests for backend parsing
