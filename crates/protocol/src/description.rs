//! Human-readable descriptions for plugins and their entity types.
//!
//! These types are used for self-documentation: plugins describe their entity
//! types, properties, and actions so that settings UIs and CLI tools can
//! display meaningful labels without hardcoding knowledge of every plugin.
//!
//! Descriptions are obtained at discovery time via `provides --describe` and
//! cached by the daemon. They contain locale-resolved strings (not translation
//! keys), so each plugin resolves translations internally before serializing.

use serde::{Deserialize, Serialize};

/// Description of a plugin and all entity types it provides.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PluginDescription {
    /// Plugin identifier (e.g. "clock", "audio", "bluez").
    pub name: String,
    /// Human-readable plugin name (e.g. "Clock", "Audio Control").
    pub display_name: String,
    /// Brief description of what the plugin does.
    pub description: String,
    /// Descriptions for each entity type this plugin provides.
    pub entity_types: Vec<EntityTypeDescription>,
}

/// Description of an entity type's schema.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct EntityTypeDescription {
    /// Entity type identifier (e.g. "audio-device", "bluetooth-adapter").
    pub entity_type: String,
    /// Human-readable name (e.g. "Audio Device", "Bluetooth Adapter").
    pub display_name: String,
    /// Brief description of what this entity represents.
    pub description: String,
    /// Descriptions of the entity's data properties.
    pub properties: Vec<PropertyDescription>,
    /// Descriptions of actions that can be triggered on this entity.
    pub actions: Vec<ActionDescription>,
}

/// Description of an entity property (a field in the JSON data).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PropertyDescription {
    /// JSON field name (e.g. "volume", "muted", "name").
    pub name: String,
    /// Human-readable label (e.g. "Volume", "Muted", "Device Name").
    pub label: String,
    /// Brief description of the property.
    pub description: String,
    /// Data type hint for UI rendering.
    pub value_type: PropertyValueType,
}

/// Type hint for property values.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum PropertyValueType {
    String,
    Bool,
    Number,
    Percent,
    Enum {
        variants: Vec<EnumVariantDescription>,
    },
    /// Nested object -- no inline description, refer to a separate entity type.
    Object,
    /// Array of values.
    Array,
}

/// Description of an enum variant.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct EnumVariantDescription {
    /// Serialized variant name (e.g. "Charging", "Output").
    pub name: String,
    /// Human-readable label (e.g. "Charging", "Audio Output").
    pub label: String,
}

/// Description of an action on an entity.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ActionDescription {
    /// Action identifier (e.g. "set-volume", "toggle-mute", "toggle").
    pub name: String,
    /// Human-readable label (e.g. "Set Volume", "Toggle Mute").
    pub label: String,
    /// Brief description of what the action does.
    pub description: String,
    /// Parameters the action accepts.
    pub params: Vec<ActionParamDescription>,
}

/// Description of an action parameter.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ActionParamDescription {
    /// Parameter name in the JSON params object (e.g. "value").
    pub name: String,
    /// Human-readable label (e.g. "Volume Level").
    pub label: String,
    /// Brief description.
    pub description: String,
    /// Whether this parameter is required.
    pub required: bool,
    /// Data type hint.
    pub value_type: PropertyValueType,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn roundtrip<T: Serialize + for<'de> Deserialize<'de> + PartialEq + std::fmt::Debug>(val: &T) {
        let json = serde_json::to_string(val).expect("serialize");
        let decoded: T = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(val, &decoded);
    }

    #[test]
    fn plugin_description_roundtrip() {
        let desc = PluginDescription {
            name: "audio".to_string(),
            display_name: "Audio Control".to_string(),
            description: "Volume control and audio device management".to_string(),
            entity_types: vec![EntityTypeDescription {
                entity_type: "audio-device".to_string(),
                display_name: "Audio Device".to_string(),
                description: "An audio input or output device".to_string(),
                properties: vec![
                    PropertyDescription {
                        name: "volume".to_string(),
                        label: "Volume".to_string(),
                        description: "Current volume level".to_string(),
                        value_type: PropertyValueType::Percent,
                    },
                    PropertyDescription {
                        name: "muted".to_string(),
                        label: "Muted".to_string(),
                        description: "Whether the device is muted".to_string(),
                        value_type: PropertyValueType::Bool,
                    },
                    PropertyDescription {
                        name: "kind".to_string(),
                        label: "Device Type".to_string(),
                        description: "Whether this is an input or output device".to_string(),
                        value_type: PropertyValueType::Enum {
                            variants: vec![
                                EnumVariantDescription {
                                    name: "Output".to_string(),
                                    label: "Audio Output".to_string(),
                                },
                                EnumVariantDescription {
                                    name: "Input".to_string(),
                                    label: "Audio Input".to_string(),
                                },
                            ],
                        },
                    },
                ],
                actions: vec![
                    ActionDescription {
                        name: "set-volume".to_string(),
                        label: "Set Volume".to_string(),
                        description: "Adjust the volume level".to_string(),
                        params: vec![ActionParamDescription {
                            name: "value".to_string(),
                            label: "Volume Level".to_string(),
                            description: "Volume as a value between 0.0 and 1.0".to_string(),
                            required: true,
                            value_type: PropertyValueType::Percent,
                        }],
                    },
                    ActionDescription {
                        name: "toggle-mute".to_string(),
                        label: "Toggle Mute".to_string(),
                        description: "Mute or unmute the device".to_string(),
                        params: vec![],
                    },
                ],
            }],
        };
        roundtrip(&desc);
    }

    #[test]
    fn entity_type_description_roundtrip() {
        let desc = EntityTypeDescription {
            entity_type: "clock".to_string(),
            display_name: "Clock".to_string(),
            description: "Current time and date".to_string(),
            properties: vec![PropertyDescription {
                name: "time".to_string(),
                label: "Time".to_string(),
                description: "Formatted time string".to_string(),
                value_type: PropertyValueType::String,
            }],
            actions: vec![],
        };
        roundtrip(&desc);
    }

    #[test]
    fn property_value_type_enum_roundtrip() {
        let val = PropertyValueType::Enum {
            variants: vec![
                EnumVariantDescription {
                    name: "Charging".to_string(),
                    label: "Charging".to_string(),
                },
                EnumVariantDescription {
                    name: "Discharging".to_string(),
                    label: "Discharging".to_string(),
                },
            ],
        };
        roundtrip(&val);
    }

    #[test]
    fn property_value_type_simple_roundtrip() {
        roundtrip(&PropertyValueType::String);
        roundtrip(&PropertyValueType::Bool);
        roundtrip(&PropertyValueType::Number);
        roundtrip(&PropertyValueType::Percent);
        roundtrip(&PropertyValueType::Object);
        roundtrip(&PropertyValueType::Array);
    }

    #[test]
    fn action_param_description_roundtrip() {
        let param = ActionParamDescription {
            name: "value".to_string(),
            label: "Brightness".to_string(),
            description: "Brightness level".to_string(),
            required: true,
            value_type: PropertyValueType::Percent,
        };
        roundtrip(&param);
    }

    #[test]
    fn empty_plugin_description() {
        let desc = PluginDescription {
            name: "test".to_string(),
            display_name: "Test Plugin".to_string(),
            description: "A test plugin".to_string(),
            entity_types: vec![],
        };
        roundtrip(&desc);
    }

    #[test]
    fn empty_entity_type_description() {
        let desc = EntityTypeDescription {
            entity_type: "test-entity".to_string(),
            display_name: "Test".to_string(),
            description: "A test entity".to_string(),
            properties: vec![],
            actions: vec![],
        };
        roundtrip(&desc);
    }
}
