## ADDED Requirements

### Requirement: Feature toggle visibility

The caffeine feature toggle SHALL only be visible when a supported screen inhibit D-Bus interface is available on the system.

#### Scenario: Toggle shown when portal available
- **WHEN** system has `org.freedesktop.portal.Desktop` service responding
- **THEN** caffeine toggle is displayed in the feature grid

#### Scenario: Toggle shown when ScreenSaver available
- **WHEN** system has `org.freedesktop.ScreenSaver` service responding
- **THEN** caffeine toggle is displayed in the feature grid

#### Scenario: Toggle hidden when no interface available
- **WHEN** neither portal nor ScreenSaver D-Bus interfaces are available
- **THEN** caffeine toggle is not displayed (plugin fails to initialize)

### Requirement: Screen inhibit activation

The system SHALL inhibit screen lock/screensaver when caffeine mode is activated via the feature toggle.

#### Scenario: Activate via portal
- **WHEN** user activates the caffeine toggle
- **AND** portal backend is in use
- **THEN** system calls `org.freedesktop.portal.Inhibit.Inhibit` with idle flag (8)
- **AND** toggle shows active state

#### Scenario: Activate via ScreenSaver
- **WHEN** user activates the caffeine toggle
- **AND** ScreenSaver backend is in use
- **THEN** system calls `org.freedesktop.ScreenSaver.Inhibit` with app name and reason
- **AND** toggle shows active state

### Requirement: Screen inhibit deactivation

The system SHALL release the screen inhibition when caffeine mode is deactivated.

#### Scenario: Deactivate via portal
- **WHEN** user deactivates the caffeine toggle
- **AND** portal backend is in use
- **THEN** system releases the inhibit handle (closes the request object)
- **AND** toggle shows inactive state

#### Scenario: Deactivate via ScreenSaver
- **WHEN** user deactivates the caffeine toggle
- **AND** ScreenSaver backend is in use
- **THEN** system calls `org.freedesktop.ScreenSaver.UnInhibit` with the stored cookie
- **AND** toggle shows inactive state

### Requirement: Backend priority

The system SHALL prefer the portal interface over ScreenSaver when both are available.

#### Scenario: Both interfaces available
- **WHEN** system has both portal and ScreenSaver interfaces available
- **THEN** system uses the portal interface for inhibition

### Requirement: Busy state during D-Bus calls

The toggle SHALL show a busy state while D-Bus operations are in progress.

#### Scenario: Show busy during activation
- **WHEN** user clicks to activate caffeine
- **THEN** toggle shows busy indicator
- **AND** busy indicator clears when D-Bus call completes

#### Scenario: Show busy during deactivation
- **WHEN** user clicks to deactivate caffeine
- **THEN** toggle shows busy indicator
- **AND** busy indicator clears when D-Bus call completes
