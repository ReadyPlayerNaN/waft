## Requirements

### Requirement: D-Bus client connects to login1 Manager
The system SHALL connect to the systemd login1 D-Bus service to execute system actions.

#### Scenario: Client uses correct D-Bus service
- **WHEN** the SystemdDbusClient is initialized
- **THEN** it SHALL connect to service `org.freedesktop.login1`
- **AND** it SHALL use object path `/org/freedesktop/login1` for Manager interface
- **AND** it SHALL use the system D-Bus bus (not session bus)

#### Scenario: Client wraps DbusHandle
- **WHEN** the SystemdDbusClient is created
- **THEN** it SHALL accept an `Arc<DbusHandle>` parameter
- **AND** it SHALL reuse this handle for all D-Bus operations
- **AND** it SHALL NOT create a separate zbus connection

### Requirement: Client resolves current session path
The system SHALL determine the current user session's D-Bus object path for session-specific operations.

#### Scenario: Session path from XDG_SESSION_ID
- **WHEN** the environment variable `XDG_SESSION_ID` is set
- **THEN** the client SHALL construct path `/org/freedesktop/login1/session/{XDG_SESSION_ID}`
- **AND** it SHALL use this path for lock and logout operations

#### Scenario: Fallback to auto session path
- **WHEN** the environment variable `XDG_SESSION_ID` is not set
- **THEN** the client SHALL use path `/org/freedesktop/login1/session/auto`
- **AND** logind SHALL resolve this to the caller's session automatically

#### Scenario: Session path is stored on initialization
- **WHEN** the SystemdDbusClient is created
- **THEN** it SHALL resolve and store the session path once
- **AND** it SHALL reuse the stored path for all session operations
- **AND** it SHALL NOT re-resolve the path on each operation

### Requirement: Client supports lock session action
The system SHALL lock the current user session when requested.

#### Scenario: Execute lock session
- **WHEN** the client executes `SystemAction::LockSession`
- **THEN** it SHALL call D-Bus method `Lock()` on the session path
- **AND** the method SHALL use interface `org.freedesktop.login1.Session`
- **AND** the call SHALL complete asynchronously via tokio

#### Scenario: Lock session succeeds
- **WHEN** the Lock() method returns successfully
- **THEN** the client SHALL return `Ok(())`
- **AND** the screen lock SHALL activate

#### Scenario: Lock session fails
- **WHEN** the Lock() method returns an error
- **THEN** the client SHALL return `Err(anyhow::Error)` with context
- **AND** the error SHALL include the D-Bus error details

### Requirement: Client supports terminate session action
The system SHALL terminate (logout) the current user session when requested.

#### Scenario: Execute terminate session
- **WHEN** the client executes `SystemAction::Terminate`
- **THEN** it SHALL call D-Bus method `Terminate()` on the session path
- **AND** the method SHALL use interface `org.freedesktop.login1.Session`
- **AND** the call SHALL complete asynchronously via tokio

#### Scenario: Terminate session succeeds
- **WHEN** the Terminate() method returns successfully
- **THEN** the client SHALL return `Ok(())`
- **AND** the user session SHALL end (logout)

#### Scenario: Terminate requires authorization
- **WHEN** the user lacks permission to terminate the session
- **THEN** D-Bus SHALL return a PolicyKit authorization error
- **AND** the client SHALL propagate this error to the caller

### Requirement: Client supports reboot action
The system SHALL reboot the machine when requested.

#### Scenario: Execute reboot with interactive flag
- **WHEN** the client executes `SystemAction::Reboot { interactive: true }`
- **THEN** it SHALL call D-Bus method `Reboot(boolean)` on `/org/freedesktop/login1`
- **AND** the method SHALL use interface `org.freedesktop.login1.Manager`
- **AND** it SHALL pass `true` as the interactive parameter
- **AND** the call SHALL complete asynchronously via tokio

#### Scenario: Reboot triggers PolicyKit prompt
- **WHEN** reboot is executed with `interactive: true`
- **AND** the user lacks permission to reboot
- **THEN** PolicyKit SHALL display an authentication dialog
- **AND** the user MAY enter credentials to authorize the action
- **AND** the client SHALL await the PolicyKit decision

#### Scenario: Reboot succeeds after authorization
- **WHEN** the user authorizes the reboot action
- **THEN** the Reboot() method SHALL return successfully
- **AND** the client SHALL return `Ok(())`
- **AND** the system SHALL initiate reboot sequence

#### Scenario: Reboot denied by PolicyKit
- **WHEN** the user denies or cancels the PolicyKit prompt
- **THEN** D-Bus SHALL return an authorization error
- **AND** the client SHALL return `Err(anyhow::Error)` with authorization context

### Requirement: Client supports power off action
The system SHALL shut down the machine when requested.

#### Scenario: Execute power off with interactive flag
- **WHEN** the client executes `SystemAction::PowerOff { interactive: true }`
- **THEN** it SHALL call D-Bus method `PowerOff(boolean)` on `/org/freedesktop/login1`
- **AND** the method SHALL use interface `org.freedesktop.login1.Manager`
- **AND** it SHALL pass `true` as the interactive parameter
- **AND** the call SHALL complete asynchronously via tokio

#### Scenario: Power off triggers PolicyKit prompt
- **WHEN** power off is executed with `interactive: true`
- **AND** the user lacks permission to shut down
- **THEN** PolicyKit SHALL display an authentication dialog
- **AND** the user MAY enter credentials to authorize the action

#### Scenario: Power off succeeds after authorization
- **WHEN** the user authorizes the power off action
- **THEN** the PowerOff() method SHALL return successfully
- **AND** the client SHALL return `Ok(())`
- **AND** the system SHALL initiate shutdown sequence

### Requirement: Client supports suspend action
The system SHALL suspend the machine when requested.

#### Scenario: Execute suspend with interactive flag
- **WHEN** the client executes `SystemAction::Suspend { interactive: true }`
- **THEN** it SHALL call D-Bus method `Suspend(boolean)` on `/org/freedesktop/login1`
- **AND** the method SHALL use interface `org.freedesktop.login1.Manager`
- **AND** it SHALL pass `true` as the interactive parameter
- **AND** the call SHALL complete asynchronously via tokio

#### Scenario: Suspend triggers PolicyKit prompt
- **WHEN** suspend is executed with `interactive: true`
- **AND** the user lacks permission to suspend
- **THEN** PolicyKit SHALL display an authentication dialog
- **AND** the user MAY enter credentials to authorize the action

#### Scenario: Suspend succeeds after authorization
- **WHEN** the user authorizes the suspend action
- **THEN** the Suspend() method SHALL return successfully
- **AND** the client SHALL return `Ok(())`
- **AND** the system SHALL enter suspend state

### Requirement: Client handles D-Bus service unavailability
The system SHALL gracefully handle cases where systemd/login1 is not available.

#### Scenario: login1 service not running
- **WHEN** SystemdDbusClient initialization attempts to connect
- **AND** the org.freedesktop.login1 service is not running
- **THEN** the client initialization SHALL fail gracefully
- **AND** it SHALL return `None` from the constructor
- **AND** it SHALL log a warning about D-Bus unavailability

#### Scenario: Plugin continues without D-Bus
- **WHEN** SystemdDbusClient initialization returns `None`
- **THEN** the SystemdActionsPlugin initialization SHALL succeed
- **AND** the plugin MAY choose not to register widgets
- **OR** the plugin MAY register disabled widgets with visual indication
- **AND** the application SHALL continue running normally

#### Scenario: D-Bus connection lost during operation
- **WHEN** a D-Bus method call fails due to connection loss
- **THEN** the client SHALL return `Err(anyhow::Error)` with connection context
- **AND** the error SHALL be propagated to the widget layer
- **AND** the widget SHALL display an error message to the user

### Requirement: Client provides typed action enum
The system SHALL use a type-safe enum for representing system actions instead of raw strings.

#### Scenario: SystemAction enum defines all actions
- **WHEN** code references a system action
- **THEN** it SHALL use the `SystemAction` enum
- **AND** the enum SHALL include variants: `LockSession`, `Terminate`, `Reboot`, `PowerOff`, `Suspend`
- **AND** power actions SHALL include an `interactive: bool` field

#### Scenario: Client executes action via enum
- **WHEN** the client's `execute_action(&self, action: SystemAction)` method is called
- **THEN** it SHALL pattern match on the action enum
- **AND** it SHALL call the appropriate D-Bus method for each variant
- **AND** it SHALL return a consistent `Result<()>` type

### Requirement: Client operations are async and non-blocking
The system SHALL perform all D-Bus operations asynchronously without blocking the GTK main thread.

#### Scenario: Action execution is async
- **WHEN** `execute_action()` is called
- **THEN** it SHALL be an async function
- **AND** it SHALL use `.await` on D-Bus method calls
- **AND** it SHALL complete on the tokio runtime

#### Scenario: Widget handlers spawn async tasks
- **WHEN** a widget button is clicked
- **THEN** the click handler SHALL spawn a tokio task for the D-Bus call
- **AND** the GTK main thread SHALL NOT block waiting for the result
- **AND** error handling SHALL occur in the spawned task

### Requirement: Client provides detailed error context
The system SHALL provide actionable error messages when operations fail.

#### Scenario: D-Bus errors include context
- **WHEN** a D-Bus method call fails
- **THEN** the error SHALL use `anyhow::Context` to add operation details
- **AND** the error message SHALL indicate which action failed (e.g., "Failed to execute reboot")
- **AND** the error SHALL include the underlying D-Bus error

#### Scenario: Authorization errors are distinguishable
- **WHEN** a PolicyKit authorization error occurs
- **THEN** the error message SHALL indicate authorization was denied
- **AND** the error SHALL distinguish between "denied" and "cancelled"
- **AND** the message SHALL guide the user toward resolution (e.g., "Contact administrator")

### Requirement: Client reuses session path resolution logic
The system SHALL reuse the session path resolution logic from the existing session monitor.

#### Scenario: Session path resolution matches SessionMonitor
- **WHEN** the SystemdDbusClient resolves the session path
- **THEN** it SHALL use the same logic as `SessionMonitor::get_session_path()`
- **AND** it SHALL check `XDG_SESSION_ID` environment variable first
- **AND** it SHALL fall back to `/session/auto` if unavailable
- **AND** this ensures consistency across session-related features
