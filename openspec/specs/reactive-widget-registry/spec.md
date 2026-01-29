## ADDED Requirements

### Requirement: Widget registration uses unique identifiers
The system SHALL identify widgets and feature toggles by unique string IDs.

#### Scenario: Widget has stable ID
- **WHEN** a plugin creates a Widget
- **THEN** the Widget struct SHALL have an `id: String` field
- **AND** the ID SHALL be unique within the registry
- **AND** the ID SHALL follow the pattern `<plugin>:<type>:<instance>` (e.g., `networkmanager:wifi:adapter-0`)

#### Scenario: Feature toggle has stable ID
- **WHEN** a plugin creates a WidgetFeatureToggle
- **THEN** the WidgetFeatureToggle struct SHALL have an `id: String` field
- **AND** the ID SHALL be unique within the registry

### Requirement: PluginRegistry supports widget subscriptions
The system SHALL allow subscribers to be notified when widgets change.

#### Scenario: Subscribe to widget changes
- **WHEN** a component calls `registry.subscribe_widgets(callback)`
- **THEN** the callback SHALL be stored
- **AND** the callback SHALL be invoked whenever widgets are registered or unregistered

#### Scenario: Subscribers are notified on registration
- **WHEN** a plugin calls `registrar.register_widget(widget)`
- **THEN** all subscribers SHALL be notified
- **AND** subsequent calls to `get_widgets_for_slot()` SHALL include the new widget

#### Scenario: Subscribers are notified on unregistration
- **WHEN** a plugin calls `registrar.unregister_widget(id)`
- **THEN** all subscribers SHALL be notified
- **AND** subsequent calls to `get_widgets_for_slot()` SHALL NOT include the removed widget

### Requirement: Plugins receive a WidgetRegistrar handle
The system SHALL provide plugins with a registration handle during element creation.

#### Scenario: Plugin receives registrar in create_elements
- **WHEN** `Plugin::create_elements()` is called
- **THEN** it SHALL receive an `Arc<dyn WidgetRegistrar>` parameter
- **AND** the plugin SHALL use this handle to register its widgets

#### Scenario: WidgetRegistrar provides registration methods
- **WHEN** a plugin has a WidgetRegistrar handle
- **THEN** it SHALL be able to call `register_widget(widget: Arc<Widget>)`
- **AND** it SHALL be able to call `register_feature_toggle(toggle: Arc<WidgetFeatureToggle>)`
- **AND** it SHALL be able to call `unregister_widget(id: &str)`
- **AND** it SHALL be able to call `unregister_feature_toggle(id: &str)`

### Requirement: Main window synchronizes UI on widget changes
The system SHALL synchronize UI containers with minimal widget remounting when widgets change.

#### Scenario: Unchanged widgets are not remounted
- **WHEN** the main window receives a widget change notification
- **THEN** it SHALL compare current container children against the new widget list by ID
- **AND** widgets present in both old and new lists SHALL NOT be removed and re-added
- **AND** only widgets no longer in the registry SHALL be removed
- **AND** only widgets newly added to the registry SHALL be appended

#### Scenario: Widget order changes use in-place reordering
- **WHEN** widget order changes (due to weight changes or new widgets inserted)
- **THEN** the container SHALL reorder existing widgets using `reorder_child_after()`
- **AND** existing widgets SHALL NOT be removed and re-added to change order

#### Scenario: Feature grid synchronizes toggles with stability
- **WHEN** the main window receives a feature toggle change notification
- **THEN** the FeatureGridWidget SHALL synchronize its displayed toggles using the same diff strategy
- **AND** unchanged toggles SHALL NOT be remounted
- **AND** it SHALL preserve menu state across synchronization

### Requirement: Static plugin methods are removed
The system SHALL NOT use static widget collection methods on the Plugin trait.

#### Scenario: Plugin trait removes get_widgets
- **WHEN** the Plugin trait is updated
- **THEN** the `get_widgets()` method SHALL be removed
- **AND** plugins SHALL register widgets via WidgetRegistrar instead

#### Scenario: Plugin trait removes get_feature_toggles
- **WHEN** the Plugin trait is updated
- **THEN** the `get_feature_toggles()` method SHALL be removed
- **AND** plugins SHALL register toggles via WidgetRegistrar instead
