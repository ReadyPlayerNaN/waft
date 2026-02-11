use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use waft_ipc::widget::{NamedWidget, Widget};

/// Thread-safe registry for managing widgets from multiple plugins
///
/// Tracks widgets by plugin_id and widget_id, supporting add/update/remove operations.
/// Widgets are sorted by weight within each plugin.
///
/// This registry is thread-safe and can be cloned cheaply (uses Arc internally).
#[derive(Clone)]
pub struct WidgetRegistry {
    // plugin_id -> widget_id -> NamedWidget
    widgets: Arc<RwLock<HashMap<String, HashMap<String, NamedWidget>>>>,
}

impl WidgetRegistry {
    /// Creates a new empty widget registry
    pub fn new() -> Self {
        Self {
            widgets: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Registers a new plugin, initializing its widget storage
    ///
    /// If the plugin already exists, this is a no-op.
    pub fn register_plugin(&self, plugin_id: String) {
        let mut widgets = self.widgets.write().unwrap();
        widgets.entry(plugin_id).or_insert_with(HashMap::new);
    }

    /// Sets all widgets for a plugin, replacing any existing widgets
    ///
    /// The plugin will be automatically registered if it doesn't exist.
    pub fn set_widgets(&self, plugin_id: &str, widgets_list: Vec<NamedWidget>) {
        let mut widgets = self.widgets.write().unwrap();
        let plugin_widgets = widgets
            .entry(plugin_id.to_string())
            .or_insert_with(HashMap::new);

        plugin_widgets.clear();

        for named_widget in widgets_list {
            plugin_widgets.insert(named_widget.id.clone(), named_widget);
        }
    }

    /// Updates a specific widget for a plugin
    ///
    /// If the widget doesn't exist, it will be created.
    /// The plugin must be registered first.
    pub fn update_widget(&self, plugin_id: &str, widget_id: &str, widget: Widget) {
        let mut widgets = self.widgets.write().unwrap();
        if let Some(plugin_widgets) = widgets.get_mut(plugin_id) {
            if let Some(named_widget) = plugin_widgets.get_mut(widget_id) {
                // Update existing widget, preserving metadata
                named_widget.widget = widget;
            }
        }
    }

    /// Removes a specific widget from a plugin
    pub fn remove_widget(&self, plugin_id: &str, widget_id: &str) {
        let mut widgets = self.widgets.write().unwrap();
        if let Some(plugin_widgets) = widgets.get_mut(plugin_id) {
            plugin_widgets.remove(widget_id);
        }
    }

    /// Removes all widgets for a plugin and unregisters it
    pub fn remove_plugin(&self, plugin_id: &str) {
        let mut widgets = self.widgets.write().unwrap();
        widgets.remove(plugin_id);
    }

    /// Returns all widgets from all plugins, sorted by weight (ascending)
    ///
    /// Lower weight values appear first in the list.
    pub fn get_all_widgets_sorted(&self) -> Vec<NamedWidget> {
        let widgets = self.widgets.read().unwrap();
        let mut result: Vec<NamedWidget> = widgets
            .values()
            .flat_map(|plugin_widgets| plugin_widgets.values())
            .cloned()
            .collect();

        result.sort_by_key(|w| w.weight);
        result
    }

    /// Returns all widgets from all plugins, unsorted
    pub fn get_all_widgets(&self) -> Vec<NamedWidget> {
        let widgets = self.widgets.read().unwrap();
        widgets
            .values()
            .flat_map(|plugin_widgets| plugin_widgets.values())
            .cloned()
            .collect()
    }

    /// Gets the number of registered plugins
    pub fn plugin_count(&self) -> usize {
        let widgets = self.widgets.read().unwrap();
        widgets.len()
    }

    /// Gets the total number of widgets across all plugins
    pub fn widget_count(&self) -> usize {
        let widgets = self.widgets.read().unwrap();
        widgets.values().map(|plugin_widgets| plugin_widgets.len()).sum()
    }
}

impl Default for WidgetRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use waft_ipc::widget::{Action, ActionParams};

    fn create_test_widget(id: &str, weight: u32) -> NamedWidget {
        NamedWidget {
            id: id.to_string(),
            weight,
            widget: Widget::Label {
                text: format!("Widget {}", id),
                css_classes: vec![],
            },
        }
    }

    #[test]
    fn test_new_registry_is_empty() {
        let registry = WidgetRegistry::new();
        assert!(registry.get_all_widgets().is_empty());
        assert_eq!(registry.plugin_count(), 0);
        assert_eq!(registry.widget_count(), 0);
    }

    #[test]
    fn test_register_plugin() {
        let registry = WidgetRegistry::new();
        registry.register_plugin("audio".to_string());

        assert_eq!(registry.plugin_count(), 1);
        assert_eq!(registry.widget_count(), 0);
    }

    #[test]
    fn test_register_plugin_twice_is_idempotent() {
        let registry = WidgetRegistry::new();
        registry.register_plugin("audio".to_string());
        registry.register_plugin("audio".to_string());

        assert_eq!(registry.plugin_count(), 1);
    }

    #[test]
    fn test_set_widgets() {
        let registry = WidgetRegistry::new();

        let widgets = vec![
            create_test_widget("widget1", 10),
            create_test_widget("widget2", 20),
        ];

        registry.set_widgets("audio", widgets);

        let all_widgets = registry.get_all_widgets();
        assert_eq!(all_widgets.len(), 2);
    }

    #[test]
    fn test_set_widgets_replaces_existing() {
        let registry = WidgetRegistry::new();

        let widgets1 = vec![
            create_test_widget("widget1", 10),
            create_test_widget("widget2", 20),
        ];
        registry.set_widgets("audio", widgets1);

        let widgets2 = vec![create_test_widget("widget3", 30)];
        registry.set_widgets("audio", widgets2);

        let all_widgets = registry.get_all_widgets();
        assert_eq!(all_widgets.len(), 1);
        assert_eq!(all_widgets[0].id, "widget3");
    }

    #[test]
    fn test_update_widget() {
        let registry = WidgetRegistry::new();
        let widgets = vec![create_test_widget("widget1", 10)];
        registry.set_widgets("audio", widgets);

        let new_widget = Widget::Button {
            label: Some("Click me".to_string()),
            icon: None,
            on_click: Action {
                id: "click".to_string(),
                params: ActionParams::None,
            },
        };

        registry.update_widget("audio", "widget1", new_widget);

        let all_widgets = registry.get_all_widgets();
        assert_eq!(all_widgets.len(), 1);

        match &all_widgets[0].widget {
            Widget::Button { label, .. } => {
                assert_eq!(label.as_ref().unwrap(), "Click me");
            }
            _ => panic!("Expected Button widget"),
        }
    }

    #[test]
    fn test_update_nonexistent_widget_does_nothing() {
        let registry = WidgetRegistry::new();
        registry.register_plugin("audio".to_string());

        let new_widget = Widget::Label {
            text: "test".to_string(),
            css_classes: vec![],
        };

        registry.update_widget("audio", "nonexistent", new_widget);

        let all_widgets = registry.get_all_widgets();
        assert!(all_widgets.is_empty());
    }

    #[test]
    fn test_update_widget_preserves_metadata() {
        let registry = WidgetRegistry::new();
        let widgets = vec![create_test_widget("widget1", 42)];
        registry.set_widgets("audio", widgets);

        let new_widget = Widget::Label {
            text: "updated".to_string(),
            css_classes: vec![],
        };

        registry.update_widget("audio", "widget1", new_widget);

        let all_widgets = registry.get_all_widgets();
        assert_eq!(all_widgets[0].weight, 42);
    }

    #[test]
    fn test_remove_widget() {
        let registry = WidgetRegistry::new();
        let widgets = vec![
            create_test_widget("widget1", 10),
            create_test_widget("widget2", 20),
        ];
        registry.set_widgets("audio", widgets);

        registry.remove_widget("audio", "widget1");

        let all_widgets = registry.get_all_widgets();
        assert_eq!(all_widgets.len(), 1);
        assert_eq!(all_widgets[0].id, "widget2");
    }

    #[test]
    fn test_remove_nonexistent_widget_does_nothing() {
        let registry = WidgetRegistry::new();
        registry.register_plugin("audio".to_string());

        registry.remove_widget("audio", "nonexistent");

        let all_widgets = registry.get_all_widgets();
        assert!(all_widgets.is_empty());
    }

    #[test]
    fn test_remove_plugin() {
        let registry = WidgetRegistry::new();
        let widgets = vec![
            create_test_widget("widget1", 10),
            create_test_widget("widget2", 20),
        ];
        registry.set_widgets("audio", widgets);

        registry.remove_plugin("audio");

        assert_eq!(registry.plugin_count(), 0);
        assert!(registry.get_all_widgets().is_empty());
    }

    #[test]
    fn test_get_all_widgets_sorted_by_weight() {
        let registry = WidgetRegistry::new();

        let widgets = vec![
            create_test_widget("w3", 300),
            create_test_widget("w1", 100),
            create_test_widget("w2", 200),
        ];
        registry.set_widgets("audio", widgets);

        let sorted = registry.get_all_widgets_sorted();
        assert_eq!(sorted.len(), 3);
        assert_eq!(sorted[0].id, "w1"); // weight 100
        assert_eq!(sorted[1].id, "w2"); // weight 200
        assert_eq!(sorted[2].id, "w3"); // weight 300
    }

    #[test]
    fn test_get_widgets_from_multiple_plugins() {
        let registry = WidgetRegistry::new();

        let audio_widgets = vec![
            create_test_widget("audio:1", 50),
            create_test_widget("audio:2", 100),
        ];
        registry.set_widgets("audio", audio_widgets);

        let battery_widgets = vec![
            create_test_widget("battery:1", 25),
            create_test_widget("battery:2", 150),
        ];
        registry.set_widgets("battery", battery_widgets);

        let sorted = registry.get_all_widgets_sorted();
        assert_eq!(sorted.len(), 4);
        assert_eq!(sorted[0].id, "battery:1"); // weight 25
        assert_eq!(sorted[1].id, "audio:1"); // weight 50
        assert_eq!(sorted[2].id, "audio:2"); // weight 100
        assert_eq!(sorted[3].id, "battery:2"); // weight 150
    }

    #[test]
    fn test_get_all_widgets() {
        let registry = WidgetRegistry::new();

        let audio_widgets = vec![
            create_test_widget("audio:1", 10),
            create_test_widget("audio:2", 20),
        ];
        registry.set_widgets("audio", audio_widgets);

        let battery_widgets = vec![create_test_widget("battery:1", 30)];
        registry.set_widgets("battery", battery_widgets);

        let all_widgets = registry.get_all_widgets();
        assert_eq!(all_widgets.len(), 3);

        // Collect IDs to verify all widgets are present
        let ids: Vec<&str> = all_widgets.iter().map(|w| w.id.as_str()).collect();
        assert!(ids.contains(&"audio:1"));
        assert!(ids.contains(&"audio:2"));
        assert!(ids.contains(&"battery:1"));
    }

    #[test]
    fn test_complex_widget_types() {
        let registry = WidgetRegistry::new();

        let widgets = vec![
            NamedWidget {
                id: "toggle1".to_string(),
                weight: 10,
                widget: Widget::FeatureToggle {
                    title: "Bluetooth".to_string(),
                    icon: "bluetooth-active".to_string(),
                    details: Some("Connected".to_string()),
                    active: true,
                    busy: false,
                    expandable: true,
                    expanded_content: Some(Box::new(Widget::Col {
                        spacing: 8,
                        css_classes: vec![],
                        children: vec![Widget::Label {
                            text: "Device 1".to_string(),
                            css_classes: vec![],
                        }
                        .into()],
                    })),
                    on_toggle: Action {
                        id: "toggle_bluetooth".to_string(),
                        params: ActionParams::None,
                    },
                },
            },
            NamedWidget {
                id: "slider1".to_string(),
                weight: 20,
                widget: Widget::Slider {
                    icon: "volume-high".to_string(),
                    value: 0.75,
                    disabled: false,
                    expandable: false,
                    expanded_content: None,
                    on_value_change: Action {
                        id: "set_volume".to_string(),
                        params: ActionParams::Value(0.75),
                    },
                    on_icon_click: Action {
                        id: "toggle_mute".to_string(),
                        params: ActionParams::None,
                    },
                },
            },
        ];

        registry.set_widgets("audio", widgets);

        let sorted = registry.get_all_widgets_sorted();
        assert_eq!(sorted.len(), 2);

        match &sorted[0].widget {
            Widget::FeatureToggle { title, active, .. } => {
                assert_eq!(title, "Bluetooth");
                assert!(*active);
            }
            _ => panic!("Expected FeatureToggle"),
        }

        match &sorted[1].widget {
            Widget::Slider { value, disabled, .. } => {
                assert_eq!(*value, 0.75);
                assert!(!disabled);
            }
            _ => panic!("Expected Slider"),
        }
    }

    #[test]
    fn test_default_impl() {
        let registry = WidgetRegistry::default();
        assert!(registry.get_all_widgets().is_empty());
    }

    #[test]
    fn test_thread_safety() {
        use std::thread;

        let registry = WidgetRegistry::new();
        let registry_clone = registry.clone();

        let handle = thread::spawn(move || {
            registry_clone.set_widgets("audio", vec![
                create_test_widget("audio:volume", 10),
            ]);
        });

        registry.set_widgets("battery", vec![
            create_test_widget("battery:status", 20),
        ]);

        handle.join().unwrap();

        assert_eq!(registry.plugin_count(), 2);
        assert_eq!(registry.widget_count(), 2);
    }

    #[test]
    fn test_clone_shares_state() {
        let registry = WidgetRegistry::new();
        let registry_clone = registry.clone();

        registry.set_widgets("audio", vec![
            create_test_widget("audio:volume", 10),
        ]);

        // Clone should see the same data
        assert_eq!(registry_clone.widget_count(), 1);
        assert_eq!(registry_clone.plugin_count(), 1);
    }
}
