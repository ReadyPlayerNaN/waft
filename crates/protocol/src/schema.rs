use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

/// JSON Schema subset used for protocol metadata.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct JsonSchema {
    #[serde(rename = "type", default, skip_serializing_if = "Option::is_none")]
    pub schema_type: Option<String>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub properties: BTreeMap<String, JsonSchema>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub required: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub items: Option<Box<JsonSchema>>,
    #[serde(rename = "enum", default, skip_serializing_if = "Option::is_none")]
    pub enum_values: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(
        rename = "additionalProperties",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub additional_properties: Option<bool>,
}

impl JsonSchema {
    pub fn object() -> Self {
        Self {
            schema_type: Some("object".to_string()),
            ..Self::default()
        }
    }

    pub fn string() -> Self {
        Self {
            schema_type: Some("string".to_string()),
            ..Self::default()
        }
    }

    pub fn boolean() -> Self {
        Self {
            schema_type: Some("boolean".to_string()),
            ..Self::default()
        }
    }

    pub fn number() -> Self {
        Self {
            schema_type: Some("number".to_string()),
            ..Self::default()
        }
    }

    pub fn array(items: JsonSchema) -> Self {
        Self {
            schema_type: Some("array".to_string()),
            items: Some(Box::new(items)),
            ..Self::default()
        }
    }

    pub fn described(mut self, description: impl Into<String>) -> Self {
        self.description = Some(description.into());
        self
    }

    pub fn with_property(
        mut self,
        name: impl Into<String>,
        schema: JsonSchema,
        required: bool,
    ) -> Self {
        let name = name.into();
        self.properties.insert(name.clone(), schema);
        if required {
            self.required.push(name);
        }
        self
    }

    pub fn with_enum(mut self, values: Vec<String>) -> Self {
        self.enum_values = Some(values);
        self
    }

    pub fn closed(mut self) -> Self {
        self.additional_properties = Some(false);
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builder_helpers_produce_expected_schema() {
        let schema = JsonSchema::object()
            .with_property("name", JsonSchema::string().described("Display name"), true)
            .with_property("enabled", JsonSchema::boolean(), true)
            .with_property("values", JsonSchema::array(JsonSchema::number()), false)
            .with_enum(vec!["alpha".to_string(), "beta".to_string()])
            .closed();

        assert_eq!(schema.schema_type.as_deref(), Some("object"));
        assert_eq!(
            schema.required,
            vec!["name".to_string(), "enabled".to_string()]
        );
        assert_eq!(
            schema.properties["name"].description.as_deref(),
            Some("Display name")
        );
        assert_eq!(
            schema.properties["values"].schema_type.as_deref(),
            Some("array")
        );
        assert_eq!(
            schema.properties["values"]
                .items
                .as_ref()
                .and_then(|i| i.schema_type.as_deref()),
            Some("number")
        );
        assert_eq!(
            schema.enum_values.as_ref().expect("enum"),
            &vec!["alpha".to_string(), "beta".to_string()]
        );
        assert_eq!(schema.additional_properties, Some(false));
    }
}
