## ADDED Requirements

### Requirement: Safe notification card dismissal

NotificationCard widgets SHALL be safely removable without triggering GTK CRITICAL assertions during dismissal.

#### Scenario: User clicks to dismiss notification card

- **WHEN** a user clicks on a notification card to dismiss it
- **THEN** the card SHALL animate out via the revealer
- **AND** the card SHALL be removed from its parent only after animation completes and event processing finishes
- **AND** no GTK CRITICAL assertions SHALL be logged

#### Scenario: User right-clicks to dismiss notification card

- **WHEN** a user right-clicks on a notification card
- **THEN** the card SHALL set `hidden = true` before starting hide animation
- **AND** subsequent gesture events SHALL be ignored

### Requirement: Safe toast widget dismissal

ToastWidget instances SHALL be safely removable without triggering GTK CRITICAL assertions during dismissal.

#### Scenario: Toast dismissed by user interaction

- **WHEN** a user clicks or right-clicks on a toast to dismiss it
- **THEN** the toast SHALL animate out via the revealer
- **AND** the toast SHALL be removed from its parent only after animation completes and event processing finishes
- **AND** no GTK CRITICAL assertions SHALL be logged

#### Scenario: Toast dismissed by TTL expiration

- **WHEN** a toast's time-to-live expires
- **THEN** the toast SHALL animate out
- **AND** removal SHALL be deferred until after animation and event processing complete

### Requirement: Single removal authority for notification widgets

Each notification widget (card or toast) SHALL have exactly one code path responsible for removing it from the widget tree.

#### Scenario: Revealer callback is the removal authority

- **WHEN** a notification widget needs to be removed
- **THEN** only the revealer's `connect_child_revealed_notify` callback SHALL perform the removal
- **AND** parent containers SHALL NOT also attempt to remove the widget

#### Scenario: Parent container hides but does not remove

- **WHEN** a parent container (notification group) updates and a card is no longer needed
- **THEN** the parent SHALL call `hide()` or `set_reveal_child(false)` on the card
- **AND** the parent SHALL NOT call `remove()` directly
