## ADDED Requirements

### Requirement: MenuItemWidget accepts widget children

The MenuItemWidget component SHALL accept any GTK widget as children content rather than individual properties.

#### Scenario: Create menu item with custom content box

- **WHEN** a consumer creates a content widget (e.g., gtk::Box with icon, labels, switch)
- **THEN** MenuItemWidget accepts it as the child parameter
- **AND** the child widget is displayed within the menu item container

#### Scenario: Flexible content structure

- **WHEN** different plugins create different content layouts
- **THEN** MenuItemWidget accepts any gtk::Widget implementation
- **AND** does not constrain the content structure

### Requirement: MenuItemWidget is always clickable

The MenuItemWidget component SHALL always be clickable with a mandatory click handler.

#### Scenario: Click handler is required

- **WHEN** creating a MenuItemWidget instance
- **THEN** the on_click callback parameter is mandatory
- **AND** the component cannot be created without a click handler

#### Scenario: Full row click invokes handler

- **WHEN** user clicks anywhere on the menu item row
- **THEN** the on_click callback is invoked
- **AND** the entire row area is clickable

#### Scenario: Visual click feedback

- **WHEN** user hovers over the menu item
- **THEN** visual hover state is displayed
- **AND** **WHEN** user clicks the menu item
- **THEN** visual active/pressed state is shown

### Requirement: MenuItemWidget applies consistent styling

The MenuItemWidget component SHALL apply consistent CSS styling across all menu items.

#### Scenario: Menu item CSS class applied

- **WHEN** MenuItemWidget is created
- **THEN** the `menu-item` CSS class is applied to the container
- **AND** consistent styling is rendered

#### Scenario: Styling does not interfere with children

- **WHEN** child widgets have their own CSS classes
- **THEN** MenuItemWidget styling does not override child styles
- **AND** child widgets render with their intended appearance

### Requirement: MenuItemWidget has minimal API surface

The MenuItemWidget component SHALL have a simple, focused API with only essential parameters.

#### Scenario: Two-parameter constructor

- **WHEN** creating a MenuItemWidget
- **THEN** only two parameters are required: child widget and click handler
- **AND** no additional configuration properties are needed

#### Scenario: No presentation logic in component

- **WHEN** MenuItemWidget is implemented
- **THEN** it contains no logic for icons, labels, or switches
- **AND** all content presentation is handled by consuming code
