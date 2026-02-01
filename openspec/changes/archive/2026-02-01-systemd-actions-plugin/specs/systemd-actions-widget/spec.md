## ADDED Requirements

### Requirement: Action group buttons appear in header
The system SHALL display two action group buttons in the main overlay header slot.

#### Scenario: Session action button is registered
- **WHEN** the systemd_actions plugin creates elements
- **THEN** it SHALL register a Widget with slot `Slot::Header`
- **AND** the widget ID SHALL be `systemd-actions:session`
- **AND** the widget weight SHALL be 100 or higher to position rightward
- **AND** the widget SHALL display a session-related icon (e.g., `system-users-symbolic`)

#### Scenario: Power action button is registered
- **WHEN** the systemd_actions plugin creates elements
- **THEN** it SHALL register a Widget with slot `Slot::Header`
- **AND** the widget ID SHALL be `systemd-actions:power`
- **AND** the widget weight SHALL be 101 or higher to position rightward
- **AND** the widget SHALL display a power-related icon (e.g., `system-shutdown-symbolic`)

### Requirement: Action buttons have expandable menus
The system SHALL provide expand buttons that reveal action menus using slide-down animation.

#### Scenario: Session button has expand control
- **WHEN** the session action widget is created
- **THEN** it SHALL include a main button area with icon and optional label
- **AND** it SHALL include an expand button with chevron icon
- **AND** it SHALL include a `gtk::Revealer` containing the action menu
- **AND** the revealer SHALL use `RevealerTransitionType::SlideDown`

#### Scenario: Power button has expand control
- **WHEN** the power action widget is created
- **THEN** it SHALL include a main button area with icon and optional label
- **AND** it SHALL include an expand button with chevron icon
- **AND** it SHALL include a `gtk::Revealer` containing the action menu
- **AND** the revealer SHALL use `RevealerTransitionType::SlideDown`

### Requirement: Expand button toggles menu visibility
The system SHALL open and close action menus when the expand button is clicked.

#### Scenario: Click expand button to open menu
- **WHEN** the user clicks the expand button on a closed action group
- **THEN** the system SHALL emit `MenuOp::OpenMenu(menu_id)` to the MenuStore
- **AND** the revealer SHALL reveal the menu content
- **AND** the chevron icon SHALL rotate to indicate expanded state

#### Scenario: Click expand button to close menu
- **WHEN** the user clicks the expand button on an open action group
- **THEN** the system SHALL emit `MenuOp::CloseMenu(menu_id)` to the MenuStore
- **AND** the revealer SHALL hide the menu content
- **AND** the chevron icon SHALL rotate to indicate collapsed state

#### Scenario: Opening one menu closes other menus
- **WHEN** the user opens a system action menu while another menu is open
- **THEN** the previously open menu SHALL close
- **AND** only the newly opened menu SHALL be visible
- **AND** this behavior SHALL be coordinated by MenuStore subscription

### Requirement: Session menu displays session actions
The system SHALL display lock and logout actions in the session action menu.

#### Scenario: Session menu contains lock action
- **WHEN** the session action menu is created
- **THEN** it SHALL contain a menu item labeled "Lock Session"
- **AND** the item SHALL display a lock icon (e.g., `system-lock-screen-symbolic`)
- **AND** the item SHALL be clickable

#### Scenario: Session menu contains logout action
- **WHEN** the session action menu is created
- **THEN** it SHALL contain a menu item labeled "Logout"
- **AND** the item SHALL display a logout icon (e.g., `system-log-out-symbolic`)
- **AND** the item SHALL be clickable

### Requirement: Power menu displays power actions
The system SHALL display reboot, shutdown, and suspend actions in the power action menu.

#### Scenario: Power menu contains reboot action
- **WHEN** the power action menu is created
- **THEN** it SHALL contain a menu item labeled "Reboot"
- **AND** the item SHALL display a reboot icon (e.g., `system-reboot-symbolic`)
- **AND** the item SHALL be clickable

#### Scenario: Power menu contains shutdown action
- **WHEN** the power action menu is created
- **THEN** it SHALL contain a menu item labeled "Shutdown"
- **AND** the item SHALL display a shutdown icon (e.g., `system-shutdown-symbolic`)
- **AND** the item SHALL be clickable

#### Scenario: Power menu contains suspend action
- **WHEN** the power action menu is created
- **THEN** it SHALL contain a menu item labeled "Suspend"
- **AND** the item SHALL display a suspend icon (e.g., `media-playback-pause-symbolic`)
- **AND** the item SHALL be clickable

### Requirement: Menu items emit action events when clicked
The system SHALL emit action selection events when menu items are clicked.

#### Scenario: Click lock action
- **WHEN** the user clicks the "Lock Session" menu item
- **THEN** the widget SHALL emit `ActionMenuOutput::ActionSelected(SystemAction::LockSession)`
- **AND** the menu SHALL remain open (action execution is separate concern)

#### Scenario: Click logout action
- **WHEN** the user clicks the "Logout" menu item
- **THEN** the widget SHALL emit `ActionMenuOutput::ActionSelected(SystemAction::Terminate)`
- **AND** the menu SHALL remain open

#### Scenario: Click reboot action
- **WHEN** the user clicks the "Reboot" menu item
- **THEN** the widget SHALL emit `ActionMenuOutput::ActionSelected(SystemAction::Reboot)`
- **AND** the menu SHALL remain open

#### Scenario: Click shutdown action
- **WHEN** the user clicks the "Shutdown" menu item
- **THEN** the widget SHALL emit `ActionMenuOutput::ActionSelected(SystemAction::PowerOff)`
- **AND** the menu SHALL remain open

#### Scenario: Click suspend action
- **WHEN** the user clicks the "Suspend" menu item
- **THEN** the widget SHALL emit `ActionMenuOutput::ActionSelected(SystemAction::Suspend)`
- **AND** the menu SHALL remain open

### Requirement: Widgets use consistent styling
The system SHALL apply CSS classes for consistent visual styling.

#### Scenario: Action group widget has CSS classes
- **WHEN** an action group widget is created
- **THEN** the root container SHALL have CSS class `system-action-group`
- **AND** the main button area SHALL have CSS class `system-action-button`
- **AND** the expand button SHALL have CSS class `expand-button`

#### Scenario: Action menu has CSS classes
- **WHEN** an action menu widget is created
- **THEN** the menu container SHALL have CSS class `system-action-menu`
- **AND** each menu item SHALL have CSS class `system-action-row`

#### Scenario: Expanded state uses CSS class
- **WHEN** an action menu is expanded
- **THEN** the action group root SHALL add CSS class `expanded`
- **AND** the chevron widget SHALL update its expanded state
- **AND** CSS transitions SHALL animate the state change

### Requirement: Widget lifecycle follows plugin pattern
The system SHALL manage widget lifecycle through plugin registration and cleanup.

#### Scenario: Widgets are registered during plugin initialization
- **WHEN** `Plugin::create_elements()` is called on SystemdActionsPlugin
- **THEN** it SHALL create two ActionGroupWidget instances
- **AND** it SHALL register both widgets via WidgetRegistrar
- **AND** each widget SHALL have a unique menu_id for MenuStore coordination

#### Scenario: Widgets handle D-Bus unavailability gracefully
- **WHEN** the D-Bus client fails to initialize
- **THEN** the plugin MAY choose not to register widgets
- **OR** the plugin MAY register disabled widgets with visual indication
- **AND** the plugin SHALL NOT crash or block initialization

### Requirement: MenuStore coordinates single-open behavior
The system SHALL ensure only one action menu is open at a time across all plugins.

#### Scenario: Subscribe to MenuStore updates
- **WHEN** an ActionGroupWidget is created with a MenuStore
- **THEN** it SHALL subscribe to MenuStore state changes
- **AND** it SHALL update its revealer and chevron based on active_menu_id
- **AND** the menu SHALL be visible when `active_menu_id == Some(this_menu_id)`

#### Scenario: Unsubscribe on widget destruction
- **WHEN** an ActionGroupWidget is destroyed
- **THEN** it SHALL clean up its MenuStore subscription
- **AND** it SHALL not receive further state updates
