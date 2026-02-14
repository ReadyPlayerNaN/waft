## ADDED Requirements

### Requirement: Sunsetr async operations run on tokio runtime

All sunsetr IPC operations that use tokio APIs SHALL be spawned on the tokio runtime, not in glib context.

#### Scenario: Toggle activate spawns on tokio

- **WHEN** user clicks sunsetr toggle to activate
- **THEN** the `spawn_start` async function SHALL execute via `tokio::spawn`
- **AND** results SHALL be communicated to glib via flume channel
- **AND** no busy-polling SHALL occur

#### Scenario: Toggle deactivate spawns on tokio

- **WHEN** user clicks sunsetr toggle to deactivate
- **THEN** the `spawn_stop` async function SHALL execute via `tokio::spawn`
- **AND** results SHALL be communicated to glib via flume channel
- **AND** no busy-polling SHALL occur

### Requirement: Sunsetr state represents process running

The sunsetr toggle active state SHALL represent whether the sunsetr process is running, regardless of current period (day/night).

#### Scenario: Process running during day

- **WHEN** sunsetr process is running and current period is "day"
- **THEN** toggle SHALL display as active ("on")
- **AND** label SHALL show "Denní režim do {time}"

#### Scenario: Process running during night

- **WHEN** sunsetr process is running and current period is "night"
- **THEN** toggle SHALL display as active ("on")
- **AND** label SHALL show "Noční světlo do {time}"

#### Scenario: Process not running

- **WHEN** sunsetr process is not running
- **THEN** toggle SHALL display as inactive ("off")
- **AND** clicking SHALL start sunsetr process

#### Scenario: Process already running, user clicks

- **WHEN** sunsetr process is already running and user clicks toggle
- **THEN** sunsetr SHALL be stopped
- **AND** toggle SHALL update to inactive state

### Requirement: Sunsetr toggle expandable when running

The sunsetr toggle SHALL be expandable with a preset menu only when the sunsetr process is running.

#### Scenario: Sunsetr running shows expand button

- **WHEN** sunsetr process is running
- **THEN** toggle SHALL have CSS class "expandable"
- **AND** expand button SHALL be visible

#### Scenario: Sunsetr not running hides expand button

- **WHEN** sunsetr process is not running
- **THEN** toggle SHALL NOT have CSS class "expandable"
- **AND** expand button SHALL be hidden via CSS

#### Scenario: Preset menu populated on expand

- **WHEN** user clicks expand button while sunsetr is running
- **THEN** menu SHALL query `sunsetr preset list --json`
- **AND** populate menu with available presets
- **AND** current preset SHALL be indicated
