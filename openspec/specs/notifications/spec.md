## ADDED Requirements

### Requirement: Toast rendering pauses during session lock

The notifications plugin SHALL pause toast rendering when the session is locked and resume when unlocked.

#### Scenario: Session locks while toasts are visible

- **WHEN** the session locks while the toast window is visible
- **THEN** the toast window SHALL be hidden immediately
- **AND** all countdown timers SHALL be paused

#### Scenario: Notification arrives during lock

- **WHEN** a notification arrives while the session is locked
- **THEN** the notification SHALL be queued in the notification store
- **AND** no toast SHALL be rendered

#### Scenario: Session unlocks with queued notifications

- **WHEN** the session unlocks and there are queued notifications
- **THEN** the toast window SHALL become visible
- **AND** queued notifications SHALL be processed for toast display

#### Scenario: Session unlocks with no queued notifications

- **WHEN** the session unlocks and there are no pending notifications
- **THEN** the toast window SHALL remain in its normal idle state

### Requirement: Toast window visibility after unlock

The toast window SHALL be in its normal operational state after session unlock, ready to display new notifications.

#### Scenario: Normal operation resumes

- **WHEN** the session unlocks
- **THEN** the toast window SHALL be ready to receive and display new notifications
- **AND** countdown timers SHALL function normally for any displayed toasts
