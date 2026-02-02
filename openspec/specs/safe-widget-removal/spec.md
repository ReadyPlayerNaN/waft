## ADDED Requirements

### Requirement: Deferred widget removal after animation

When a GTK widget is being removed as part of an animation completion callback, the removal SHALL be deferred using `glib::idle_add_local_once` to ensure all event handlers have completed processing.

#### Scenario: Widget removed after revealer animation completes

- **WHEN** a revealer's `connect_child_revealed_notify` callback fires with `is_child_revealed() == false`
- **THEN** the widget removal SHALL be scheduled via `idle_add_local_once`
- **AND** the removal SHALL execute after current GTK event processing completes

#### Scenario: Gesture handler fires during animation

- **WHEN** a gesture handler is processing events while the widget is animating out
- **THEN** the gesture handler SHALL complete before widget removal occurs
- **AND** no GTK CRITICAL assertions SHALL be triggered

### Requirement: Hidden flag guards gesture handlers

Gesture handlers on dismissable widgets SHALL check a `hidden` flag before accessing widget properties that could fail on destroyed widgets.

#### Scenario: Gesture handler on already-hidden widget

- **WHEN** a gesture handler fires on a widget that has `hidden == true`
- **THEN** the handler SHALL return early without accessing widget properties
- **AND** no `widget.pick()` or `widget.parent()` calls SHALL be made

#### Scenario: Gesture handler sets hidden flag before animation

- **WHEN** a gesture handler initiates widget dismissal
- **THEN** the handler SHALL set `hidden = true` before starting the hide animation
- **AND** subsequent gesture events on the same widget SHALL be ignored
