## ADDED Requirements

### Requirement: Display discovery at initialization
The plugin SHALL discover all controllable displays during initialization by querying available backends (brightnessctl for backlight devices, ddcutil for DDC/CI monitors).

#### Scenario: System has backlight devices
- **WHEN** brightnessctl is installed and reports backlight devices
- **THEN** plugin SHALL include those devices in the controllable displays list

#### Scenario: System has DDC/CI-capable monitors
- **WHEN** ddcutil is installed and detects monitors supporting DDC/CI
- **THEN** plugin SHALL include those monitors in the controllable displays list

#### Scenario: No controllable displays found
- **WHEN** no backends report any controllable displays
- **THEN** plugin SHALL register no widgets and log a debug message

#### Scenario: Backend tool not installed
- **WHEN** a backend CLI tool (brightnessctl or ddcutil) is not installed
- **THEN** plugin SHALL skip that backend and continue with other available backends

### Requirement: Single master slider in Controls slot
The plugin SHALL create one master SliderControlWidget in the Controls slot with weight 60, positioned after microphone controls.

#### Scenario: Plugin registers master slider
- **WHEN** at least one controllable display is discovered
- **THEN** plugin SHALL register exactly one slider widget in the Controls slot

#### Scenario: Master slider icon
- **WHEN** master slider is displayed
- **THEN** slider SHALL show `display-brightness-symbolic` icon

#### Scenario: Icon click behavior
- **WHEN** user clicks the master slider icon
- **THEN** no action SHALL occur (brightness has no mute equivalent)

### Requirement: Master slider shows average brightness
The master slider value SHALL display the arithmetic average of all controllable displays' current brightness values.

#### Scenario: Multiple displays at different levels
- **WHEN** display A is at 50% and display B is at 90%
- **THEN** master slider SHALL show 70% (average)

#### Scenario: Individual display changes
- **WHEN** user adjusts an individual display's brightness via the menu
- **THEN** master slider value SHALL update to reflect the new average

### Requirement: Master slider proportional scaling
When user adjusts the master slider, all displays SHALL scale proportionally using the formula: `new_value = current_value × (new_master / old_master)`.

#### Scenario: Scale displays up proportionally
- **WHEN** displays are at A=25%, B=45% (master=35%) and user drags master to 70%
- **THEN** displays SHALL become A=50%, B=90%

#### Scenario: Scale displays down proportionally
- **WHEN** displays are at A=50%, B=90% (master=70%) and user drags master to 35%
- **THEN** displays SHALL become A=25%, B=45%

#### Scenario: Master slider to zero
- **WHEN** user drags master slider to 0%
- **THEN** all displays SHALL be set to 0% brightness

#### Scenario: Recovery from zero
- **WHEN** all displays are at 0% and user drags master to 50%
- **THEN** all displays SHALL be set to 50% (additive scaling when old_master=0)

### Requirement: Expandable menu with per-display sliders
When two or more controllable displays exist, the master slider SHALL include an expandable menu containing individual sliders for each display.

#### Scenario: Multiple displays show menu
- **WHEN** plugin discovers 2 or more controllable displays
- **THEN** master slider SHALL show expand button revealing per-display sliders

#### Scenario: Single display hides menu
- **WHEN** plugin discovers exactly one controllable display
- **THEN** master slider SHALL NOT show expand button (no menu needed)

#### Scenario: Menu row contents
- **WHEN** expandable menu is shown
- **THEN** each row SHALL contain: display type icon, brightness slider, truncated display name

#### Scenario: Menu ordering
- **WHEN** expandable menu contains multiple displays
- **THEN** displays SHALL be ordered: backlight devices first, then external monitors, alphabetically within each group

### Requirement: Per-display slider shows actual brightness
Individual sliders in the expandable menu SHALL display and control the actual current brightness of each display.

#### Scenario: Individual slider reflects actual value
- **WHEN** display A has actual brightness of 25%
- **THEN** display A's slider in the menu SHALL show 25%

#### Scenario: Individual slider adjustment
- **WHEN** user drags display A's slider to 40%
- **THEN** display A's brightness SHALL be set to 40% and master slider SHALL update to new average

### Requirement: Display type icons in menu
Per-display sliders in the menu SHALL show icons appropriate to the display type.

#### Scenario: Backlight device icon
- **WHEN** a menu row represents a backlight device (via brightnessctl)
- **THEN** row SHALL display `display-brightness-symbolic` icon

#### Scenario: External monitor icon
- **WHEN** a menu row represents an external monitor (via ddcutil)
- **THEN** row SHALL display `video-display-symbolic` icon

### Requirement: Initial brightness state
When creating widgets, the plugin SHALL query current brightness from backends and initialize all sliders to reflect current display state.

#### Scenario: Initial master value
- **WHEN** displays are currently at A=60%, B=80%
- **THEN** master slider SHALL initialize at 70% (average)

#### Scenario: Initial individual values
- **WHEN** displays are currently at A=60%, B=80%
- **THEN** individual sliders SHALL initialize at 60% and 80% respectively

### Requirement: Graceful backend failure handling
If a backend call fails when setting brightness, the plugin SHALL log the error and continue operation.

#### Scenario: Partial failure during master adjustment
- **WHEN** master slider adjusts multiple displays and one backend call fails
- **THEN** plugin SHALL log the error for failed display, apply changes to successful displays, and update master to reflect actual state

#### Scenario: Individual slider failure
- **WHEN** setting brightness fails for an individual display
- **THEN** plugin SHALL log the error and leave that slider unchanged
