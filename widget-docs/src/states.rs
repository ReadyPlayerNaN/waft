use waft_ui_gtk::types::{Action, ActionParams, Widget};

/// Represents a single state of a widget to document
pub struct WidgetState {
    pub name: String,
    pub filename: String,
    pub description: String,
    pub widget: Widget,
}

/// Collection of all states for a specific widget type
pub struct WidgetStates {
    pub name: String,
    pub description: String,
    pub states: Vec<WidgetState>,
}

impl WidgetStates {
    /// Get all widget types and their states
    pub fn all() -> Vec<Self> {
        vec![
            Self::feature_toggle(),
            Self::slider(),
            Self::menu_row(),
            Self::col_and_row(),
            Self::switch(),
            Self::spinner(),
            Self::checkmark(),
            Self::button(),
            Self::label(),
        ]
    }

    fn feature_toggle() -> Self {
        Self {
            name: "feature-toggle".to_string(),
            description: "A toggleable feature card with optional expanded content".to_string(),
            states: vec![
                WidgetState {
                    name: "Inactive".to_string(),
                    filename: "inactive".to_string(),
                    description: "Basic inactive state".to_string(),
                    widget: Widget::FeatureToggle {
                        title: "Bluetooth".to_string(),
                        icon: "bluetooth-symbolic".to_string(),
                        details: None,
                        active: false,
                        busy: false,
                        expandable: false,
                        expanded_content: None,
                        on_toggle: dummy_action(),
                    },
                },
                WidgetState {
                    name: "Active".to_string(),
                    filename: "active".to_string(),
                    description: "Active state (feature is enabled)".to_string(),
                    widget: Widget::FeatureToggle {
                        title: "Bluetooth".to_string(),
                        icon: "bluetooth-active-symbolic".to_string(),
                        details: None,
                        active: true,
                        busy: false,
                        expandable: false,
                        expanded_content: None,
                        on_toggle: dummy_action(),
                    },
                },
                WidgetState {
                    name: "With Details".to_string(),
                    filename: "with_details".to_string(),
                    description: "Active with details text".to_string(),
                    widget: Widget::FeatureToggle {
                        title: "Bluetooth".to_string(),
                        icon: "bluetooth-active-symbolic".to_string(),
                        details: Some("Connected to 2 devices".to_string()),
                        active: true,
                        busy: false,
                        expandable: false,
                        expanded_content: None,
                        on_toggle: dummy_action(),
                    },
                },
                WidgetState {
                    name: "Busy".to_string(),
                    filename: "busy".to_string(),
                    description: "Busy state (operation in progress)".to_string(),
                    widget: Widget::FeatureToggle {
                        title: "Bluetooth".to_string(),
                        icon: "bluetooth-symbolic".to_string(),
                        details: Some("Connecting...".to_string()),
                        active: false,
                        busy: true,
                        expandable: false,
                        expanded_content: None,
                        on_toggle: dummy_action(),
                    },
                },
                WidgetState {
                    name: "Expandable".to_string(),
                    filename: "expandable".to_string(),
                    description: "Inactive but expandable (shows chevron)".to_string(),
                    widget: Widget::FeatureToggle {
                        title: "Bluetooth".to_string(),
                        icon: "bluetooth-symbolic".to_string(),
                        details: None,
                        active: false,
                        busy: false,
                        expandable: true,
                        expanded_content: Some(Box::new(Widget::Label {
                            text: "Expanded content".to_string(),
                            css_classes: vec![],
                        })),
                        on_toggle: dummy_action(),
                    },
                },
                WidgetState {
                    name: "Active Expandable".to_string(),
                    filename: "active_expandable".to_string(),
                    description: "Active and expandable".to_string(),
                    widget: Widget::FeatureToggle {
                        title: "Bluetooth".to_string(),
                        icon: "bluetooth-active-symbolic".to_string(),
                        details: Some("Connected".to_string()),
                        active: true,
                        busy: false,
                        expandable: true,
                        expanded_content: Some(Box::new(Widget::Col {
                            spacing: 4,
                            css_classes: vec![],
                            children: vec![
                                Widget::MenuRow {
                                    icon: Some("audio-headphones-symbolic".to_string()),
                                    label: "Headphones".to_string(),
                                    trailing: None,
                                    sensitive: true,
                        busy: false,
                                    on_click: Some(dummy_action()),
                                }.into(),
                            ],
                        })),
                        on_toggle: dummy_action(),
                    },
                },
                WidgetState {
                    name: "Expanded".to_string(),
                    filename: "expanded".to_string(),
                    description: "Expanded state showing child widgets (note: expansion state is handled by MenuStore at runtime)".to_string(),
                    widget: Widget::FeatureToggle {
                        title: "Bluetooth".to_string(),
                        icon: "bluetooth-active-symbolic".to_string(),
                        details: Some("Connected".to_string()),
                        active: true,
                        busy: false,
                        expandable: true,
                        expanded_content: Some(Box::new(Widget::Col {
                            spacing: 4,
                            css_classes: vec![],
                            children: vec![
                                Widget::MenuRow {
                                    icon: Some("audio-headphones-symbolic".to_string()),
                                    label: "Headphones".to_string(),
                                    trailing: Some(Box::new(Widget::Checkmark { visible: true })),
                                    sensitive: true,
                        busy: false,
                                    on_click: Some(dummy_action()),
                                }.into(),
                                Widget::MenuRow {
                                    icon: Some("phone-symbolic".to_string()),
                                    label: "Phone".to_string(),
                                    trailing: None,
                                    sensitive: true,
                        busy: false,
                                    on_click: Some(dummy_action()),
                                }.into(),
                            ],
                        })),
                        on_toggle: dummy_action(),
                    },
                },
            ],
        }
    }

    fn slider() -> Self {
        Self {
            name: "slider".to_string(),
            description: "A slider control with icon and optional expanded content".to_string(),
            states: vec![
                WidgetState {
                    name: "Normal".to_string(),
                    filename: "normal".to_string(),
                    description: "Normal state with value".to_string(),
                    widget: Widget::Slider {
                        icon: "audio-volume-high-symbolic".to_string(),
                        value: 0.65,
                        disabled: false,
                        expandable: false,
                        expanded_content: None,
                        on_value_change: dummy_action(),
                        on_icon_click: dummy_action(),
                    },
                },
                WidgetState {
                    name: "Disabled".to_string(),
                    filename: "disabled".to_string(),
                    description: "Disabled state (dims the slider row)".to_string(),
                    widget: Widget::Slider {
                        icon: "audio-volume-muted-symbolic".to_string(),
                        value: 0.0,
                        disabled: true,
                        expandable: false,
                        expanded_content: None,
                        on_value_change: dummy_action(),
                        on_icon_click: dummy_action(),
                    },
                },
                WidgetState {
                    name: "Expandable".to_string(),
                    filename: "expandable".to_string(),
                    description: "Expandable slider with child content".to_string(),
                    widget: Widget::Slider {
                        icon: "audio-volume-high-symbolic".to_string(),
                        value: 0.75,
                        disabled: false,
                        expandable: true,
                        expanded_content: Some(Box::new(Widget::Col {
                            spacing: 4,
                            css_classes: vec![],
                            children: vec![
                                Widget::MenuRow {
                                    icon: Some("audio-headphones-symbolic".to_string()),
                                    label: "Output Device".to_string(),
                                    trailing: None,
                                    sensitive: true,
                        busy: false,
                                    on_click: Some(dummy_action()),
                                }.into(),
                            ],
                        })),
                        on_value_change: dummy_action(),
                        on_icon_click: dummy_action(),
                    },
                },
            ],
        }
    }

    fn menu_row() -> Self {
        Self {
            name: "menu-row".to_string(),
            description: "A menu row with icon, labels, and optional trailing widget".to_string(),
            states: vec![
                WidgetState {
                    name: "Basic".to_string(),
                    filename: "basic".to_string(),
                    description: "Basic row with label only".to_string(),
                    widget: Widget::MenuRow {
                        icon: None,
                        label: "Settings".to_string(),
                        trailing: None,
                        sensitive: true,
                        busy: false,
                        on_click: Some(dummy_action()),
                    },
                },
                WidgetState {
                    name: "With Icon".to_string(),
                    filename: "with_icon".to_string(),
                    description: "Row with icon".to_string(),
                    widget: Widget::MenuRow {
                        icon: Some("settings-symbolic".to_string()),
                        label: "Settings".to_string(),
                        trailing: None,
                        sensitive: true,
                        busy: false,
                        on_click: Some(dummy_action()),
                    },
                },
                WidgetState {
                    name: "With Sublabel".to_string(),
                    filename: "with_sublabel".to_string(),
                    description: "Row with icon and sublabel".to_string(),
                    widget: Widget::MenuRow {
                        icon: Some("network-wireless-symbolic".to_string()),
                        label: "Wi-Fi Network".to_string(),
                        trailing: None,
                        sensitive: true,
                        busy: false,
                        on_click: Some(dummy_action()),
                    },
                },
                WidgetState {
                    name: "With Switch".to_string(),
                    filename: "with_switch".to_string(),
                    description: "Row with trailing switch".to_string(),
                    widget: Widget::MenuRow {
                        icon: Some("airplane-mode-symbolic".to_string()),
                        label: "Airplane Mode".to_string(),
                        trailing: Some(Box::new(Widget::Switch {
                            active: false,
                            sensitive: true,
                            on_toggle: dummy_action(),
                        })),
                        sensitive: true,
                        busy: false,
                        on_click: None,
                    },
                },
                WidgetState {
                    name: "With Checkmark".to_string(),
                    filename: "with_checkmark".to_string(),
                    description: "Row with trailing checkmark".to_string(),
                    widget: Widget::MenuRow {
                        icon: Some("audio-headphones-symbolic".to_string()),
                        label: "Default Device".to_string(),
                        trailing: Some(Box::new(Widget::Checkmark { visible: true })),
                        sensitive: true,
                        busy: false,
                        on_click: Some(dummy_action()),
                    },
                },
                WidgetState {
                    name: "With Spinner".to_string(),
                    filename: "with_spinner".to_string(),
                    description: "Row with trailing spinner".to_string(),
                    widget: Widget::MenuRow {
                        icon: Some("network-wireless-symbolic".to_string()),
                        label: "Connecting...".to_string(),
                        trailing: Some(Box::new(Widget::Spinner { spinning: true })),
                        sensitive: false,
                        busy: false,
                        on_click: None,
                    },
                },
                WidgetState {
                    name: "Insensitive".to_string(),
                    filename: "insensitive".to_string(),
                    description: "Disabled/insensitive row".to_string(),
                    widget: Widget::MenuRow {
                        icon: Some("network-wired-symbolic".to_string()),
                        label: "Wired Connection".to_string(),
                        trailing: None,
                        sensitive: false,
                        busy: false,
                        on_click: Some(dummy_action()),
                    },
                },
            ],
        }
    }

    fn col_and_row() -> Self {
        Self {
            name: "col-and-row".to_string(),
            description: "Layout containers for organizing child widgets".to_string(),
            states: vec![
                WidgetState {
                    name: "Col (Vertical)".to_string(),
                    filename: "col".to_string(),
                    description: "Vertical layout with spacing".to_string(),
                    widget: Widget::Col {
                        spacing: 8,
                        css_classes: vec![],
                        children: vec![
                            Widget::Label {
                                text: "Item 1".to_string(),
                                css_classes: vec![],
                            }.into(),
                            Widget::Label {
                                text: "Item 2".to_string(),
                                css_classes: vec![],
                            }.into(),
                            Widget::Label {
                                text: "Item 3".to_string(),
                                css_classes: vec![],
                            }.into(),
                        ],
                    },
                },
                WidgetState {
                    name: "Row (Horizontal)".to_string(),
                    filename: "row".to_string(),
                    description: "Horizontal layout with spacing".to_string(),
                    widget: Widget::Row {
                        spacing: 12,
                        css_classes: vec![],
                        children: vec![
                            Widget::Button {
                                label: Some("Button 1".to_string()),
                                icon: None,
                                on_click: dummy_action(),
                            }.into(),
                            Widget::Button {
                                label: Some("Button 2".to_string()),
                                icon: None,
                                on_click: dummy_action(),
                            }.into(),
                        ],
                    },
                },
            ],
        }
    }

    fn switch() -> Self {
        Self {
            name: "switch".to_string(),
            description: "A simple toggle switch".to_string(),
            states: vec![
                WidgetState {
                    name: "Off".to_string(),
                    filename: "off".to_string(),
                    description: "Switch in off state".to_string(),
                    widget: Widget::Switch {
                        active: false,
                        sensitive: true,
                        on_toggle: dummy_action(),
                    },
                },
                WidgetState {
                    name: "On".to_string(),
                    filename: "on".to_string(),
                    description: "Switch in on state".to_string(),
                    widget: Widget::Switch {
                        active: true,
                        sensitive: true,
                        on_toggle: dummy_action(),
                    },
                },
                WidgetState {
                    name: "Insensitive".to_string(),
                    filename: "insensitive".to_string(),
                    description: "Disabled switch".to_string(),
                    widget: Widget::Switch {
                        active: false,
                        sensitive: false,
                        on_toggle: dummy_action(),
                    },
                },
            ],
        }
    }

    fn spinner() -> Self {
        Self {
            name: "spinner".to_string(),
            description: "A loading spinner".to_string(),
            states: vec![
                WidgetState {
                    name: "Spinning".to_string(),
                    filename: "spinning".to_string(),
                    description: "Active spinner".to_string(),
                    widget: Widget::Spinner { spinning: true },
                },
                WidgetState {
                    name: "Stopped".to_string(),
                    filename: "stopped".to_string(),
                    description: "Inactive spinner".to_string(),
                    widget: Widget::Spinner { spinning: false },
                },
            ],
        }
    }

    fn checkmark() -> Self {
        Self {
            name: "checkmark".to_string(),
            description: "A checkmark indicator".to_string(),
            states: vec![
                WidgetState {
                    name: "Visible".to_string(),
                    filename: "visible".to_string(),
                    description: "Visible checkmark".to_string(),
                    widget: Widget::Checkmark { visible: true },
                },
                WidgetState {
                    name: "Hidden".to_string(),
                    filename: "hidden".to_string(),
                    description: "Hidden checkmark".to_string(),
                    widget: Widget::Checkmark { visible: false },
                },
            ],
        }
    }

    fn button() -> Self {
        Self {
            name: "button".to_string(),
            description: "A clickable button".to_string(),
            states: vec![
                WidgetState {
                    name: "Label Only".to_string(),
                    filename: "label_only".to_string(),
                    description: "Button with text label".to_string(),
                    widget: Widget::Button {
                        label: Some("Click Me".to_string()),
                        icon: None,
                        on_click: dummy_action(),
                    },
                },
                WidgetState {
                    name: "Icon Only".to_string(),
                    filename: "icon_only".to_string(),
                    description: "Button with icon only".to_string(),
                    widget: Widget::Button {
                        label: None,
                        icon: Some("list-add-symbolic".to_string()),
                        on_click: dummy_action(),
                    },
                },
                WidgetState {
                    name: "Icon and Label".to_string(),
                    filename: "icon_and_label".to_string(),
                    description: "Button with both icon and label".to_string(),
                    widget: Widget::Button {
                        label: Some("Add Item".to_string()),
                        icon: Some("list-add-symbolic".to_string()),
                        on_click: dummy_action(),
                    },
                },
            ],
        }
    }

    fn label() -> Self {
        Self {
            name: "label".to_string(),
            description: "A text label".to_string(),
            states: vec![
                WidgetState {
                    name: "Plain".to_string(),
                    filename: "plain".to_string(),
                    description: "Plain text label".to_string(),
                    widget: Widget::Label {
                        text: "Hello, World!".to_string(),
                        css_classes: vec![],
                    },
                },
                WidgetState {
                    name: "With CSS Classes".to_string(),
                    filename: "with_css".to_string(),
                    description: "Label with CSS classes applied".to_string(),
                    widget: Widget::Label {
                        text: "Styled Label".to_string(),
                        css_classes: vec!["title".to_string(), "bold".to_string()],
                    },
                },
            ],
        }
    }
}

fn dummy_action() -> Action {
    Action {
        id: "dummy".to_string(),
        params: ActionParams::None,
    }
}
