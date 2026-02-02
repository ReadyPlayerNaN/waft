## ADDED Requirements

### Requirement: Unified Feature Toggle component

There SHALL be a single FeatureToggle component that supports both simple and expandable variants via CSS classes.

#### Scenario: Simple toggle renders without expand button

- **WHEN** FeatureToggle is created without expandable option
- **THEN** component SHALL render `<Box><MainButton /><ExpandButton /></Box>`
- **AND** Box SHALL NOT have CSS class "expandable"
- **AND** ExpandButton SHALL be hidden via CSS

#### Scenario: Expandable toggle shows expand button

- **WHEN** FeatureToggle is created with expandable option
- **THEN** component SHALL render `<Box><MainButton /><ExpandButton /></Box>`
- **AND** Box SHALL have CSS class "expandable"
- **AND** ExpandButton SHALL be visible

#### Scenario: Toggle switches from simple to expandable

- **WHEN** FeatureToggle expandable state changes from false to true
- **THEN** CSS class "expandable" SHALL be added to Box
- **AND** ExpandButton SHALL become visible via CSS
- **AND** no widget rebuilding SHALL occur

#### Scenario: Toggle switches from expandable to simple

- **WHEN** FeatureToggle expandable state changes from true to false
- **THEN** CSS class "expandable" SHALL be removed from Box
- **AND** ExpandButton SHALL become hidden via CSS
- **AND** no widget rebuilding SHALL occur
