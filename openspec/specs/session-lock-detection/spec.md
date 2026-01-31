## ADDED Requirements

### Requirement: Session lock detection via logind

The system SHALL detect session lock and unlock events by subscribing to the `org.freedesktop.login1.Session` D-Bus interface on the system bus.

#### Scenario: Session locks

- **WHEN** logind emits the `Lock` signal on the session's D-Bus path
- **THEN** the session monitor SHALL broadcast a lock event to all plugins

#### Scenario: Session unlocks

- **WHEN** logind emits the `Unlock` signal on the session's D-Bus path
- **THEN** the session monitor SHALL broadcast an unlock event to all plugins

#### Scenario: D-Bus connection unavailable

- **WHEN** the system bus is not available or logind is not running
- **THEN** the session monitor SHALL log a warning and continue without session detection
- **AND** the application SHALL function normally without lock/unlock awareness

### Requirement: Plugin lifecycle hooks for session state

The Plugin trait SHALL provide optional lifecycle hooks that plugins can implement to respond to session state changes.

#### Scenario: Plugin receives lock notification

- **WHEN** a session lock event is broadcast
- **THEN** all registered plugins SHALL have their `on_session_lock` method called

#### Scenario: Plugin receives unlock notification

- **WHEN** a session unlock event is broadcast
- **THEN** all registered plugins SHALL have their `on_session_unlock` method called

#### Scenario: Plugin does not implement hooks

- **WHEN** a plugin does not override the lifecycle hooks
- **THEN** the default implementation SHALL be a no-op

### Requirement: Main window pauses on lock

The main window SHALL pause all animations and ensure a clean hidden state when the session locks.

#### Scenario: Animation running during lock

- **WHEN** the session locks while an animation is in progress
- **THEN** the animation SHALL be stopped immediately
- **AND** the window SHALL be forced to hidden state without animation

#### Scenario: Window visible during lock

- **WHEN** the session locks while the main overlay is visible
- **THEN** the window SHALL be hidden immediately

#### Scenario: Unlock restores clean state

- **WHEN** the session unlocks
- **THEN** the main overlay SHALL remain hidden
- **AND** animation state SHALL be reset to initial values (progress = 0.0)
- **AND** IPC commands SHALL resume normal operation
