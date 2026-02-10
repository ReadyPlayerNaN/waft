//! Widget builder helpers for ergonomic widget construction.
//!
//! This module provides builder patterns for common widgets, making it easier
//! to construct complex widget hierarchies with sensible defaults.
//!
//! # Example
//!
//! ```rust
//! use waft_plugin_sdk::builder::*;
//!
//! let widget = FeatureToggleBuilder::new("Bluetooth")
//!     .icon("bluetooth-active-symbolic")
//!     .details("Connected to 2 devices")
//!     .active(true)
//!     .expandable(true)
//!     .on_toggle("toggle_bluetooth")
//!     .build();
//! ```

use waft_ipc::widget::{Action, ActionParams, Node, StatusOption, Widget};

/// Builder for FeatureToggle widgets (most commonly used).
///
/// # Example
///
/// ```rust
/// use waft_plugin_sdk::builder::FeatureToggleBuilder;
///
/// let toggle = FeatureToggleBuilder::new("Wi-Fi")
///     .icon("network-wireless-symbolic")
///     .active(true)
///     .on_toggle("toggle_wifi")
///     .build();
/// ```
#[derive(Debug, Clone)]
pub struct FeatureToggleBuilder {
    title: String,
    icon: String,
    details: Option<String>,
    active: bool,
    busy: bool,
    expandable: bool,
    expanded_content: Option<Box<Widget>>,
    on_toggle: Action,
}

impl FeatureToggleBuilder {
    /// Create a new FeatureToggle builder with the given title.
    ///
    /// # Defaults
    /// - icon: "emblem-system-symbolic"
    /// - active: false
    /// - busy: false
    /// - expandable: false
    /// - on_toggle: Action with id "toggle" and no params
    pub fn new(title: impl Into<String>) -> Self {
        Self {
            title: title.into(),
            icon: "emblem-system-symbolic".into(),
            details: None,
            active: false,
            busy: false,
            expandable: false,
            expanded_content: None,
            on_toggle: Action {
                id: "toggle".into(),
                params: ActionParams::None,
            },
        }
    }

    /// Set the icon name (themed icon or path).
    pub fn icon(mut self, icon: impl Into<String>) -> Self {
        self.icon = icon.into();
        self
    }

    /// Set optional details text shown below the title.
    pub fn details(mut self, details: impl Into<String>) -> Self {
        self.details = Some(details.into());
        self
    }

    /// Set the active state of the toggle.
    pub fn active(mut self, active: bool) -> Self {
        self.active = active;
        self
    }

    /// Set the busy state (shows spinner instead of toggle).
    pub fn busy(mut self, busy: bool) -> Self {
        self.busy = busy;
        self
    }

    /// Set whether the widget can be expanded.
    pub fn expandable(mut self, expandable: bool) -> Self {
        self.expandable = expandable;
        self
    }

    /// Set the expanded content widget.
    pub fn expanded_content(mut self, content: Widget) -> Self {
        self.expanded_content = Some(Box::new(content));
        self.expandable = true; // Auto-enable expandable if content is set
        self
    }

    /// Set the toggle action by ID (params will be None).
    pub fn on_toggle(mut self, action_id: impl Into<String>) -> Self {
        self.on_toggle = Action {
            id: action_id.into(),
            params: ActionParams::None,
        };
        self
    }

    /// Set the full toggle action.
    pub fn on_toggle_action(mut self, action: Action) -> Self {
        self.on_toggle = action;
        self
    }

    /// Build the FeatureToggle widget.
    pub fn build(self) -> Widget {
        Widget::FeatureToggle {
            title: self.title,
            icon: self.icon,
            details: self.details,
            active: self.active,
            busy: self.busy,
            expandable: self.expandable,
            expanded_content: self.expanded_content,
            on_toggle: self.on_toggle,
        }
    }
}

/// Builder for Slider widgets.
///
/// # Example
///
/// ```rust
/// use waft_plugin_sdk::builder::SliderBuilder;
///
/// let slider = SliderBuilder::new(0.75)
///     .icon("audio-volume-high-symbolic")
///     .on_value_change("set_volume")
///     .on_icon_click("toggle_mute")
///     .build();
/// ```
#[derive(Debug, Clone)]
pub struct SliderBuilder {
    icon: String,
    value: f64,
    muted: bool,
    expandable: bool,
    expanded_content: Option<Box<Widget>>,
    on_value_change: Action,
    on_icon_click: Action,
}

impl SliderBuilder {
    /// Create a new Slider builder with the given value (0.0 to 1.0).
    ///
    /// # Defaults
    /// - icon: "emblem-system-symbolic"
    /// - muted: false
    /// - expandable: false
    /// - on_value_change: Action with id "value_change" and Value param
    /// - on_icon_click: Action with id "icon_click" and no params
    pub fn new(value: f64) -> Self {
        Self {
            icon: "emblem-system-symbolic".into(),
            value: value.clamp(0.0, 1.0),
            muted: false,
            expandable: false,
            expanded_content: None,
            on_value_change: Action {
                id: "value_change".into(),
                params: ActionParams::Value(value),
            },
            on_icon_click: Action {
                id: "icon_click".into(),
                params: ActionParams::None,
            },
        }
    }

    /// Set the icon name (themed icon or path).
    pub fn icon(mut self, icon: impl Into<String>) -> Self {
        self.icon = icon.into();
        self
    }

    /// Set the muted state (semantic state, renderer picks icon).
    pub fn muted(mut self, muted: bool) -> Self {
        self.muted = muted;
        self
    }

    /// Set whether the widget can be expanded.
    pub fn expandable(mut self, expandable: bool) -> Self {
        self.expandable = expandable;
        self
    }

    /// Set the expanded content widget.
    pub fn expanded_content(mut self, content: Widget) -> Self {
        self.expanded_content = Some(Box::new(content));
        self.expandable = true;
        self
    }

    /// Set the value change action by ID.
    pub fn on_value_change(mut self, action_id: impl Into<String>) -> Self {
        self.on_value_change = Action {
            id: action_id.into(),
            params: ActionParams::Value(self.value),
        };
        self
    }

    /// Set the full value change action.
    pub fn on_value_change_action(mut self, action: Action) -> Self {
        self.on_value_change = action;
        self
    }

    /// Set the icon click action by ID.
    pub fn on_icon_click(mut self, action_id: impl Into<String>) -> Self {
        self.on_icon_click = Action {
            id: action_id.into(),
            params: ActionParams::None,
        };
        self
    }

    /// Set the full icon click action.
    pub fn on_icon_click_action(mut self, action: Action) -> Self {
        self.on_icon_click = action;
        self
    }

    /// Build the Slider widget.
    pub fn build(self) -> Widget {
        Widget::Slider {
            icon: self.icon,
            value: self.value,
            muted: self.muted,
            expandable: self.expandable,
            expanded_content: self.expanded_content,
            on_value_change: self.on_value_change,
            on_icon_click: self.on_icon_click,
        }
    }
}

/// Builder for MenuRow widgets.
///
/// # Example
///
/// ```rust
/// use waft_plugin_sdk::builder::MenuRowBuilder;
///
/// let row = MenuRowBuilder::new("Settings")
///     .icon("preferences-system-symbolic")
///     .sublabel("Configure system")
///     .on_click("open_settings")
///     .build();
/// ```
#[derive(Debug, Clone)]
pub struct MenuRowBuilder {
    icon: Option<String>,
    label: String,
    sublabel: Option<String>,
    trailing: Option<Box<Widget>>,
    sensitive: bool,
    on_click: Option<Action>,
}

impl MenuRowBuilder {
    /// Create a new MenuRow builder with the given label.
    ///
    /// # Defaults
    /// - icon: None
    /// - sublabel: None
    /// - trailing: None
    /// - sensitive: true
    /// - on_click: None
    pub fn new(label: impl Into<String>) -> Self {
        Self {
            icon: None,
            label: label.into(),
            sublabel: None,
            trailing: None,
            sensitive: true,
            on_click: None,
        }
    }

    /// Set the icon name (themed icon or path).
    pub fn icon(mut self, icon: impl Into<String>) -> Self {
        self.icon = Some(icon.into());
        self
    }

    /// Set the sublabel text shown below the main label.
    pub fn sublabel(mut self, sublabel: impl Into<String>) -> Self {
        self.sublabel = Some(sublabel.into());
        self
    }

    /// Set the trailing widget (Switch, Spinner, Checkmark).
    pub fn trailing(mut self, widget: Widget) -> Self {
        self.trailing = Some(Box::new(widget));
        self
    }

    /// Set whether the row is sensitive (clickable).
    pub fn sensitive(mut self, sensitive: bool) -> Self {
        self.sensitive = sensitive;
        self
    }

    /// Set the click action by ID.
    pub fn on_click(mut self, action_id: impl Into<String>) -> Self {
        self.on_click = Some(Action {
            id: action_id.into(),
            params: ActionParams::None,
        });
        self
    }

    /// Set the full click action.
    pub fn on_click_action(mut self, action: Action) -> Self {
        self.on_click = Some(action);
        self
    }

    /// Build the MenuRow widget.
    pub fn build(self) -> Widget {
        Widget::MenuRow {
            icon: self.icon,
            label: self.label,
            sublabel: self.sublabel,
            trailing: self.trailing,
            sensitive: self.sensitive,
            on_click: self.on_click,
        }
    }
}

/// Builder for Switch widgets.
///
/// # Example
///
/// ```rust
/// use waft_plugin_sdk::builder::SwitchBuilder;
///
/// let switch = SwitchBuilder::new()
///     .active(true)
///     .on_toggle("toggle_feature")
///     .build();
/// ```
#[derive(Debug, Clone)]
pub struct SwitchBuilder {
    active: bool,
    sensitive: bool,
    on_toggle: Action,
}

impl SwitchBuilder {
    /// Create a new Switch builder.
    ///
    /// # Defaults
    /// - active: false
    /// - sensitive: true
    /// - on_toggle: Action with id "toggle" and no params
    pub fn new() -> Self {
        Self {
            active: false,
            sensitive: true,
            on_toggle: Action {
                id: "toggle".into(),
                params: ActionParams::None,
            },
        }
    }

    /// Set the active state of the switch.
    pub fn active(mut self, active: bool) -> Self {
        self.active = active;
        self
    }

    /// Set whether the switch is sensitive (can be toggled).
    pub fn sensitive(mut self, sensitive: bool) -> Self {
        self.sensitive = sensitive;
        self
    }

    /// Set the toggle action by ID.
    pub fn on_toggle(mut self, action_id: impl Into<String>) -> Self {
        self.on_toggle = Action {
            id: action_id.into(),
            params: ActionParams::None,
        };
        self
    }

    /// Set the full toggle action.
    pub fn on_toggle_action(mut self, action: Action) -> Self {
        self.on_toggle = action;
        self
    }

    /// Build the Switch widget.
    pub fn build(self) -> Widget {
        Widget::Switch {
            active: self.active,
            sensitive: self.sensitive,
            on_toggle: self.on_toggle,
        }
    }
}

impl Default for SwitchBuilder {
    fn default() -> Self {
        Self::new()
    }
}

/// Builder for Button widgets.
///
/// # Example
///
/// ```rust
/// use waft_plugin_sdk::builder::ButtonBuilder;
///
/// let button = ButtonBuilder::new()
///     .label("Power Off")
///     .icon("system-shutdown-symbolic")
///     .on_click("shutdown")
///     .build();
/// ```
#[derive(Debug, Clone)]
pub struct ButtonBuilder {
    label: Option<String>,
    icon: Option<String>,
    on_click: Action,
}

impl ButtonBuilder {
    /// Create a new Button builder.
    ///
    /// # Defaults
    /// - label: None
    /// - icon: None
    /// - on_click: Action with id "click" and no params
    pub fn new() -> Self {
        Self {
            label: None,
            icon: None,
            on_click: Action {
                id: "click".into(),
                params: ActionParams::None,
            },
        }
    }

    /// Set the button label text.
    pub fn label(mut self, label: impl Into<String>) -> Self {
        self.label = Some(label.into());
        self
    }

    /// Set the button icon name.
    pub fn icon(mut self, icon: impl Into<String>) -> Self {
        self.icon = Some(icon.into());
        self
    }

    /// Set the click action by ID.
    pub fn on_click(mut self, action_id: impl Into<String>) -> Self {
        self.on_click = Action {
            id: action_id.into(),
            params: ActionParams::None,
        };
        self
    }

    /// Set the full click action.
    pub fn on_click_action(mut self, action: Action) -> Self {
        self.on_click = action;
        self
    }

    /// Build the Button widget.
    pub fn build(self) -> Widget {
        Widget::Button {
            label: self.label,
            icon: self.icon,
            on_click: self.on_click,
        }
    }
}

impl Default for ButtonBuilder {
    fn default() -> Self {
        Self::new()
    }
}

/// Builder for Label widgets.
///
/// # Example
///
/// ```rust
/// use waft_plugin_sdk::builder::LabelBuilder;
///
/// let label = LabelBuilder::new("Hello World")
///     .css_class("title")
///     .css_class("bold")
///     .build();
/// ```
#[derive(Debug, Clone)]
pub struct LabelBuilder {
    text: String,
    css_classes: Vec<String>,
}

impl LabelBuilder {
    /// Create a new Label builder with the given text.
    ///
    /// # Defaults
    /// - css_classes: empty
    pub fn new(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            css_classes: Vec::new(),
        }
    }

    /// Add a CSS class to the label.
    pub fn css_class(mut self, class: impl Into<String>) -> Self {
        self.css_classes.push(class.into());
        self
    }

    /// Add multiple CSS classes to the label.
    pub fn css_classes(mut self, classes: Vec<String>) -> Self {
        self.css_classes.extend(classes);
        self
    }

    /// Build the Label widget.
    pub fn build(self) -> Widget {
        Widget::Label {
            text: self.text,
            css_classes: self.css_classes,
        }
    }
}

/// Builder for InfoCard widgets (display-only).
///
/// # Example
///
/// ```rust
/// use waft_plugin_sdk::builder::InfoCardBuilder;
///
/// let card = InfoCardBuilder::new("Sunny")
///     .icon("weather-clear-symbolic")
///     .description("25°C, clear skies")
///     .build();
/// ```
#[derive(Debug, Clone)]
pub struct InfoCardBuilder {
    icon: String,
    title: String,
    description: Option<String>,
    on_click: Option<Action>,
}

impl InfoCardBuilder {
    /// Create a new InfoCard builder with the given title.
    ///
    /// # Defaults
    /// - icon: "emblem-system-symbolic"
    /// - description: None
    /// - on_click: None
    pub fn new(title: impl Into<String>) -> Self {
        Self {
            icon: "emblem-system-symbolic".into(),
            title: title.into(),
            description: None,
            on_click: None,
        }
    }

    /// Set the icon name (themed icon or path).
    pub fn icon(mut self, icon: impl Into<String>) -> Self {
        self.icon = icon.into();
        self
    }

    /// Set the description text shown below the title.
    pub fn description(mut self, description: impl Into<String>) -> Self {
        self.description = Some(description.into());
        self
    }

    /// Set the click action for the card.
    pub fn on_click(mut self, action_id: impl Into<String>) -> Self {
        self.on_click = Some(Action {
            id: action_id.into(),
            params: ActionParams::None,
        });
        self
    }

    /// Build the InfoCard widget.
    pub fn build(self) -> Widget {
        Widget::InfoCard {
            icon: self.icon,
            title: self.title,
            description: self.description,
            on_click: self.on_click,
        }
    }
}

/// Builder for Row widgets (horizontal layout).
///
/// # Example
///
/// ```rust
/// use waft_plugin_sdk::builder::{RowBuilder, LabelBuilder};
///
/// let row = RowBuilder::new()
///     .spacing(8)
///     .child(LabelBuilder::new("Left").build())
///     .child(LabelBuilder::new("Right").build())
///     .build();
/// ```
#[derive(Debug, Clone)]
pub struct RowBuilder {
    spacing: u32,
    css_classes: Vec<String>,
    children: Vec<Node>,
}

impl RowBuilder {
    /// Create a new Row builder.
    pub fn new() -> Self {
        Self {
            spacing: 0,
            css_classes: Vec::new(),
            children: Vec::new(),
        }
    }

    /// Set spacing between children.
    pub fn spacing(mut self, spacing: u32) -> Self {
        self.spacing = spacing;
        self
    }

    /// Add a CSS class.
    pub fn css_class(mut self, class: impl Into<String>) -> Self {
        self.css_classes.push(class.into());
        self
    }

    /// Add a child widget.
    pub fn child(mut self, widget: Widget) -> Self {
        self.children.push(Node::from(widget));
        self
    }

    /// Add a keyed child widget.
    pub fn keyed_child(mut self, key: impl Into<String>, widget: Widget) -> Self {
        self.children.push(Node::keyed(key, widget));
        self
    }

    /// Add multiple child widgets.
    pub fn children(mut self, widgets: Vec<Widget>) -> Self {
        self.children.extend(widgets.into_iter().map(Node::from));
        self
    }

    /// Build the Row widget.
    pub fn build(self) -> Widget {
        Widget::Row {
            spacing: self.spacing,
            css_classes: self.css_classes,
            children: self.children,
        }
    }
}

impl Default for RowBuilder {
    fn default() -> Self {
        Self::new()
    }
}

/// Builder for Col widgets (vertical layout).
///
/// # Example
///
/// ```rust
/// use waft_plugin_sdk::builder::{ColBuilder, LabelBuilder};
///
/// let col = ColBuilder::new()
///     .spacing(4)
///     .child(LabelBuilder::new("Top").build())
///     .child(LabelBuilder::new("Bottom").build())
///     .build();
/// ```
#[derive(Debug, Clone)]
pub struct ColBuilder {
    spacing: u32,
    css_classes: Vec<String>,
    children: Vec<Node>,
}

impl ColBuilder {
    /// Create a new Col builder.
    pub fn new() -> Self {
        Self {
            spacing: 0,
            css_classes: Vec::new(),
            children: Vec::new(),
        }
    }

    /// Set spacing between children.
    pub fn spacing(mut self, spacing: u32) -> Self {
        self.spacing = spacing;
        self
    }

    /// Add a CSS class.
    pub fn css_class(mut self, class: impl Into<String>) -> Self {
        self.css_classes.push(class.into());
        self
    }

    /// Add a child widget.
    pub fn child(mut self, widget: Widget) -> Self {
        self.children.push(Node::from(widget));
        self
    }

    /// Add a keyed child widget.
    pub fn keyed_child(mut self, key: impl Into<String>, widget: Widget) -> Self {
        self.children.push(Node::keyed(key, widget));
        self
    }

    /// Add multiple child widgets.
    pub fn children(mut self, widgets: Vec<Widget>) -> Self {
        self.children.extend(widgets.into_iter().map(Node::from));
        self
    }

    /// Build the Col widget.
    pub fn build(self) -> Widget {
        Widget::Col {
            spacing: self.spacing,
            css_classes: self.css_classes,
            children: self.children,
        }
    }
}

impl Default for ColBuilder {
    fn default() -> Self {
        Self::new()
    }
}

/// Builder for StatusCycleButton widgets.
///
/// A button that cycles through a list of options, displaying the current
/// option's label and icon.
///
/// # Example
///
/// ```rust
/// use waft_plugin_sdk::builder::StatusCycleButtonBuilder;
/// use waft_ipc::widget::StatusOption;
///
/// let button = StatusCycleButtonBuilder::new("set_mode")
///     .icon("preferences-system-symbolic")
///     .option("auto", "Auto")
///     .option("manual", "Manual")
///     .value("auto")
///     .build();
/// ```
#[derive(Debug, Clone)]
pub struct StatusCycleButtonBuilder {
    value: String,
    icon: String,
    options: Vec<StatusOption>,
    on_cycle: Action,
}

impl StatusCycleButtonBuilder {
    /// Create a new StatusCycleButton builder with the given action ID.
    pub fn new(action_id: impl Into<String>) -> Self {
        Self {
            value: String::new(),
            icon: "emblem-system-symbolic".into(),
            options: Vec::new(),
            on_cycle: Action {
                id: action_id.into(),
                params: ActionParams::None,
            },
        }
    }

    /// Set the current value (should match one of the option IDs).
    pub fn value(mut self, value: impl Into<String>) -> Self {
        self.value = value.into();
        self
    }

    /// Set the icon name.
    pub fn icon(mut self, icon: impl Into<String>) -> Self {
        self.icon = icon.into();
        self
    }

    /// Add an option with ID and display label.
    pub fn option(mut self, id: impl Into<String>, label: impl Into<String>) -> Self {
        self.options.push(StatusOption {
            id: id.into(),
            label: label.into(),
        });
        self
    }

    /// Set all options at once.
    pub fn options(mut self, options: Vec<StatusOption>) -> Self {
        self.options = options;
        self
    }

    /// Build the StatusCycleButton widget.
    pub fn build(self) -> Widget {
        Widget::StatusCycleButton {
            value: self.value,
            icon: self.icon,
            options: self.options,
            on_cycle: self.on_cycle,
        }
    }
}

/// Builder for ListRow widgets (horizontal row of children with CSS classes).
///
/// # Example
///
/// ```rust
/// use waft_plugin_sdk::builder::{ListRowBuilder, LabelBuilder};
///
/// let row = ListRowBuilder::new()
///     .css_class("event-row")
///     .child(LabelBuilder::new("Event").build())
///     .build();
/// ```
#[derive(Debug, Clone)]
pub struct ListRowBuilder {
    children: Vec<Node>,
    css_classes: Vec<String>,
}

impl ListRowBuilder {
    /// Create a new ListRow builder.
    pub fn new() -> Self {
        Self {
            children: Vec::new(),
            css_classes: Vec::new(),
        }
    }

    /// Add a child widget.
    pub fn child(mut self, widget: Widget) -> Self {
        self.children.push(Node::from(widget));
        self
    }

    /// Add a keyed child widget.
    pub fn keyed_child(mut self, key: impl Into<String>, widget: Widget) -> Self {
        self.children.push(Node::keyed(key, widget));
        self
    }

    /// Add a CSS class.
    pub fn css_class(mut self, class: impl Into<String>) -> Self {
        self.css_classes.push(class.into());
        self
    }

    /// Build the ListRow widget.
    pub fn build(self) -> Widget {
        Widget::ListRow {
            children: self.children,
            css_classes: self.css_classes,
        }
    }
}

impl Default for ListRowBuilder {
    fn default() -> Self {
        Self::new()
    }
}

/// Builder for IconList widgets (icon + vertical list of children).
///
/// # Example
///
/// ```rust
/// use waft_plugin_sdk::builder::{IconListBuilder, LabelBuilder};
///
/// let list = IconListBuilder::new("calendar-symbolic")
///     .icon_size(24)
///     .child(LabelBuilder::new("Meeting at 10am").build())
///     .child(LabelBuilder::new("Lunch at noon").build())
///     .build();
/// ```
#[derive(Debug, Clone)]
pub struct IconListBuilder {
    icon: String,
    icon_size: i32,
    children: Vec<Node>,
}

impl IconListBuilder {
    /// Create a new IconList builder with the given icon name.
    ///
    /// # Defaults
    /// - icon_size: 16
    /// - children: empty
    pub fn new(icon: impl Into<String>) -> Self {
        Self {
            icon: icon.into(),
            icon_size: 16,
            children: Vec::new(),
        }
    }

    /// Set the icon size in pixels.
    pub fn icon_size(mut self, size: i32) -> Self {
        self.icon_size = size;
        self
    }

    /// Add a child widget.
    pub fn child(mut self, widget: Widget) -> Self {
        self.children.push(Node::from(widget));
        self
    }

    /// Add a keyed child widget.
    pub fn keyed_child(mut self, key: impl Into<String>, widget: Widget) -> Self {
        self.children.push(Node::keyed(key, widget));
        self
    }

    /// Build the IconList widget.
    pub fn build(self) -> Widget {
        Widget::IconList {
            icon: self.icon,
            icon_size: self.icon_size,
            children: self.children,
        }
    }
}

/// Builder for ListButton widgets (flat button for use in lists).
///
/// # Example
///
/// ```rust
/// use waft_plugin_sdk::builder::ListButtonBuilder;
///
/// let button = ListButtonBuilder::new("Open Calendar")
///     .icon("x-office-calendar-symbolic")
///     .on_click("open_calendar")
///     .build();
/// ```
#[derive(Debug, Clone)]
pub struct ListButtonBuilder {
    label: String,
    icon: Option<String>,
    css_classes: Vec<String>,
    on_click: Action,
}

impl ListButtonBuilder {
    /// Create a new ListButton builder with the given label.
    ///
    /// # Defaults
    /// - icon: None
    /// - css_classes: empty
    /// - on_click: Action with id "click" and no params
    pub fn new(label: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            icon: None,
            css_classes: Vec::new(),
            on_click: Action {
                id: "click".into(),
                params: ActionParams::None,
            },
        }
    }

    /// Set the icon name.
    pub fn icon(mut self, icon: impl Into<String>) -> Self {
        self.icon = Some(icon.into());
        self
    }

    /// Add a CSS class.
    pub fn css_class(mut self, class: impl Into<String>) -> Self {
        self.css_classes.push(class.into());
        self
    }

    /// Set the click action by ID.
    pub fn on_click(mut self, action_id: impl Into<String>) -> Self {
        self.on_click = Action {
            id: action_id.into(),
            params: ActionParams::None,
        };
        self
    }

    /// Set the full click action.
    pub fn on_click_action(mut self, action: Action) -> Self {
        self.on_click = action;
        self
    }

    /// Build the ListButton widget.
    pub fn build(self) -> Widget {
        Widget::ListButton {
            label: self.label,
            icon: self.icon,
            css_classes: self.css_classes,
            on_click: self.on_click,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_feature_toggle_builder_minimal() {
        let widget = FeatureToggleBuilder::new("Bluetooth").build();

        match widget {
            Widget::FeatureToggle {
                title,
                icon,
                active,
                busy,
                expandable,
                ..
            } => {
                assert_eq!(title, "Bluetooth");
                assert_eq!(icon, "emblem-system-symbolic");
                assert!(!active);
                assert!(!busy);
                assert!(!expandable);
            }
            _ => panic!("Expected FeatureToggle"),
        }
    }

    #[test]
    fn test_feature_toggle_builder_full() {
        let widget = FeatureToggleBuilder::new("Wi-Fi")
            .icon("network-wireless-symbolic")
            .details("Connected")
            .active(true)
            .expandable(true)
            .on_toggle("toggle_wifi")
            .build();

        match widget {
            Widget::FeatureToggle {
                title,
                icon,
                details,
                active,
                on_toggle,
                ..
            } => {
                assert_eq!(title, "Wi-Fi");
                assert_eq!(icon, "network-wireless-symbolic");
                assert_eq!(details, Some("Connected".to_string()));
                assert!(active);
                assert_eq!(on_toggle.id, "toggle_wifi");
            }
            _ => panic!("Expected FeatureToggle"),
        }
    }

    #[test]
    fn test_feature_toggle_builder_auto_expandable() {
        let inner = LabelBuilder::new("Content").build();
        let widget = FeatureToggleBuilder::new("Test")
            .expanded_content(inner)
            .build();

        match widget {
            Widget::FeatureToggle {
                expandable,
                expanded_content,
                ..
            } => {
                assert!(expandable); // Auto-enabled
                assert!(expanded_content.is_some());
            }
            _ => panic!("Expected FeatureToggle"),
        }
    }

    #[test]
    fn test_slider_builder_minimal() {
        let widget = SliderBuilder::new(0.5).build();

        match widget {
            Widget::Slider {
                value,
                muted,
                expandable,
                ..
            } => {
                assert_eq!(value, 0.5);
                assert!(!muted);
                assert!(!expandable);
            }
            _ => panic!("Expected Slider"),
        }
    }

    #[test]
    fn test_slider_builder_full() {
        let widget = SliderBuilder::new(0.75)
            .icon("audio-volume-high-symbolic")
            .muted(false)
            .on_value_change("set_volume")
            .on_icon_click("toggle_mute")
            .build();

        match widget {
            Widget::Slider {
                icon,
                value,
                on_value_change,
                on_icon_click,
                ..
            } => {
                assert_eq!(icon, "audio-volume-high-symbolic");
                assert_eq!(value, 0.75);
                assert_eq!(on_value_change.id, "set_volume");
                assert_eq!(on_icon_click.id, "toggle_mute");
            }
            _ => panic!("Expected Slider"),
        }
    }

    #[test]
    fn test_slider_builder_clamps_value() {
        let widget_low = SliderBuilder::new(-0.5).build();
        let widget_high = SliderBuilder::new(1.5).build();

        match widget_low {
            Widget::Slider { value, .. } => assert_eq!(value, 0.0),
            _ => panic!("Expected Slider"),
        }

        match widget_high {
            Widget::Slider { value, .. } => assert_eq!(value, 1.0),
            _ => panic!("Expected Slider"),
        }
    }

    #[test]
    fn test_menu_row_builder_minimal() {
        let widget = MenuRowBuilder::new("Settings").build();

        match widget {
            Widget::MenuRow {
                label,
                icon,
                sublabel,
                sensitive,
                ..
            } => {
                assert_eq!(label, "Settings");
                assert!(icon.is_none());
                assert!(sublabel.is_none());
                assert!(sensitive);
            }
            _ => panic!("Expected MenuRow"),
        }
    }

    #[test]
    fn test_menu_row_builder_full() {
        let switch = SwitchBuilder::new().active(true).build();
        let widget = MenuRowBuilder::new("Feature")
            .icon("preferences-system-symbolic")
            .sublabel("Enable feature")
            .trailing(switch)
            .on_click("toggle_feature")
            .build();

        match widget {
            Widget::MenuRow {
                icon,
                label,
                sublabel,
                trailing,
                on_click,
                ..
            } => {
                assert_eq!(icon, Some("preferences-system-symbolic".to_string()));
                assert_eq!(label, "Feature");
                assert_eq!(sublabel, Some("Enable feature".to_string()));
                assert!(trailing.is_some());
                assert!(on_click.is_some());
            }
            _ => panic!("Expected MenuRow"),
        }
    }

    #[test]
    fn test_switch_builder() {
        let widget = SwitchBuilder::new()
            .active(true)
            .on_toggle("toggle_feature")
            .build();

        match widget {
            Widget::Switch {
                active,
                sensitive,
                on_toggle,
            } => {
                assert!(active);
                assert!(sensitive);
                assert_eq!(on_toggle.id, "toggle_feature");
            }
            _ => panic!("Expected Switch"),
        }
    }

    #[test]
    fn test_button_builder_minimal() {
        let widget = ButtonBuilder::new().build();

        match widget {
            Widget::Button {
                label,
                icon,
                on_click,
            } => {
                assert!(label.is_none());
                assert!(icon.is_none());
                assert_eq!(on_click.id, "click");
            }
            _ => panic!("Expected Button"),
        }
    }

    #[test]
    fn test_button_builder_full() {
        let widget = ButtonBuilder::new()
            .label("Power Off")
            .icon("system-shutdown-symbolic")
            .on_click("shutdown")
            .build();

        match widget {
            Widget::Button {
                label,
                icon,
                on_click,
            } => {
                assert_eq!(label, Some("Power Off".to_string()));
                assert_eq!(icon, Some("system-shutdown-symbolic".to_string()));
                assert_eq!(on_click.id, "shutdown");
            }
            _ => panic!("Expected Button"),
        }
    }

    #[test]
    fn test_label_builder() {
        let widget = LabelBuilder::new("Hello World")
            .css_class("title")
            .css_class("bold")
            .build();

        match widget {
            Widget::Label { text, css_classes } => {
                assert_eq!(text, "Hello World");
                assert_eq!(css_classes, vec!["title", "bold"]);
            }
            _ => panic!("Expected Label"),
        }
    }

    #[test]
    fn test_info_card_builder_minimal() {
        let widget = InfoCardBuilder::new("Status").build();

        match widget {
            Widget::InfoCard {
                icon,
                title,
                description,
                on_click,
            } => {
                assert_eq!(title, "Status");
                assert_eq!(icon, "emblem-system-symbolic");
                assert!(description.is_none());
                assert!(on_click.is_none());
            }
            _ => panic!("Expected InfoCard"),
        }
    }

    #[test]
    fn test_info_card_builder_full() {
        let widget = InfoCardBuilder::new("Sunny")
            .icon("weather-clear-symbolic")
            .description("25°C, clear skies")
            .build();

        match widget {
            Widget::InfoCard {
                icon,
                title,
                description,
                ..
            } => {
                assert_eq!(icon, "weather-clear-symbolic");
                assert_eq!(title, "Sunny");
                assert_eq!(description, Some("25°C, clear skies".to_string()));
            }
            _ => panic!("Expected InfoCard"),
        }
    }

    #[test]
    fn test_info_card_builder_clickable() {
        let widget = InfoCardBuilder::new("Click me")
            .icon("go-next-symbolic")
            .on_click("open_details")
            .build();

        match widget {
            Widget::InfoCard { on_click, .. } => {
                assert!(on_click.is_some());
                assert_eq!(on_click.unwrap().id, "open_details");
            }
            _ => panic!("Expected InfoCard"),
        }
    }

    #[test]
    fn test_nested_builder_usage() {
        // Demonstrate building a complex nested structure
        let expanded_menu = ColBuilder::new()
            .spacing(4)
            .child(
                MenuRowBuilder::new("Device 1")
                    .icon("bluetooth-symbolic")
                    .trailing(SwitchBuilder::new().active(true).build())
                    .build(),
            )
            .child(
                MenuRowBuilder::new("Device 2")
                    .icon("bluetooth-symbolic")
                    .trailing(SwitchBuilder::new().active(false).build())
                    .build(),
            )
            .build();

        let widget = FeatureToggleBuilder::new("Bluetooth")
            .icon("bluetooth-active-symbolic")
            .active(true)
            .expanded_content(expanded_menu)
            .on_toggle("toggle_bluetooth")
            .build();

        match widget {
            Widget::FeatureToggle {
                title,
                expandable,
                expanded_content,
                ..
            } => {
                assert_eq!(title, "Bluetooth");
                assert!(expandable);
                assert!(expanded_content.is_some());

                match *expanded_content.unwrap() {
                    Widget::Col { children, .. } => {
                        assert_eq!(children.len(), 2);
                    }
                    _ => panic!("Expected Col"),
                }
            }
            _ => panic!("Expected FeatureToggle"),
        }
    }

    #[test]
    fn test_list_row_builder() {
        let widget = ListRowBuilder::new()
            .css_class("event-row")
            .child(LabelBuilder::new("Left").build())
            .child(LabelBuilder::new("Right").build())
            .build();

        match widget {
            Widget::ListRow {
                children,
                css_classes,
            } => {
                assert_eq!(children.len(), 2);
                assert_eq!(css_classes, vec!["event-row"]);
            }
            _ => panic!("Expected ListRow"),
        }
    }

    #[test]
    fn test_icon_list_builder() {
        let widget = IconListBuilder::new("calendar-symbolic")
            .icon_size(24)
            .child(LabelBuilder::new("Event 1").build())
            .child(LabelBuilder::new("Event 2").build())
            .build();

        match widget {
            Widget::IconList {
                icon,
                icon_size,
                children,
            } => {
                assert_eq!(icon, "calendar-symbolic");
                assert_eq!(icon_size, 24);
                assert_eq!(children.len(), 2);
            }
            _ => panic!("Expected IconList"),
        }
    }

    #[test]
    fn test_icon_list_builder_defaults() {
        let widget = IconListBuilder::new("test-icon").build();

        match widget {
            Widget::IconList {
                icon_size,
                children,
                ..
            } => {
                assert_eq!(icon_size, 16);
                assert!(children.is_empty());
            }
            _ => panic!("Expected IconList"),
        }
    }

    #[test]
    fn test_list_button_builder() {
        let widget = ListButtonBuilder::new("Open Calendar")
            .icon("x-office-calendar-symbolic")
            .css_class("suggested-action")
            .on_click("open_calendar")
            .build();

        match widget {
            Widget::ListButton {
                label,
                icon,
                css_classes,
                on_click,
            } => {
                assert_eq!(label, "Open Calendar");
                assert_eq!(icon, Some("x-office-calendar-symbolic".to_string()));
                assert_eq!(css_classes, vec!["suggested-action"]);
                assert_eq!(on_click.id, "open_calendar");
            }
            _ => panic!("Expected ListButton"),
        }
    }

    #[test]
    fn test_list_button_builder_minimal() {
        let widget = ListButtonBuilder::new("Click me").build();

        match widget {
            Widget::ListButton {
                label,
                icon,
                css_classes,
                on_click,
            } => {
                assert_eq!(label, "Click me");
                assert!(icon.is_none());
                assert!(css_classes.is_empty());
                assert_eq!(on_click.id, "click");
            }
            _ => panic!("Expected ListButton"),
        }
    }
}
