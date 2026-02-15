use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use waft_protocol::entity::display::{
    ConfigSchema, Constraints, FieldSchema, FieldState, FieldType,
};

/// Darkman YAML configuration structure.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct DarkmanYamlConfig {
    pub lat: Option<f64>,
    pub lng: Option<f64>,
    pub usegeoclue: Option<bool>,
    pub dbusserver: Option<bool>,
    pub portal: Option<bool>,
}

fn config_path() -> Result<PathBuf> {
    Ok(dirs::config_dir()
        .ok_or_else(|| anyhow::anyhow!("No config directory"))?
        .join("darkman/config.yaml"))
}

/// Parse darkman config from ~/.config/darkman/config.yaml.
pub fn parse_darkman_config() -> Result<DarkmanYamlConfig> {
    let path = config_path()?;

    if !path.exists() {
        log::warn!("[darkman-config] Config file not found, using defaults");
        return Ok(DarkmanYamlConfig::default());
    }

    let yaml_str = std::fs::read_to_string(&path).context("Failed to read config file")?;

    if yaml_str.trim().is_empty() {
        return Ok(DarkmanYamlConfig::default());
    }

    match serde_yaml::from_str(&yaml_str) {
        Ok(config) => Ok(config),
        Err(e) => {
            log::error!("[darkman-config] Failed to parse config.yaml: {}", e);
            Err(anyhow::anyhow!("Config file has syntax errors: {}", e))
        }
    }
}

/// Write darkman config to ~/.config/darkman/config.yaml.
pub fn write_darkman_config(config: &DarkmanYamlConfig) -> Result<()> {
    let path = config_path()?;

    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).context("Failed to create darkman config directory")?;
    }

    let yaml_str = serde_yaml::to_string(config).context("Failed to serialize config to YAML")?;

    std::fs::write(&path, yaml_str).context("Failed to write config file")?;

    Ok(())
}

/// Build schema metadata for darkman's 5 configuration fields.
pub fn build_schema() -> ConfigSchema {
    let mut fields = HashMap::new();

    fields.insert(
        "latitude".into(),
        FieldSchema {
            available: true,
            state: FieldState::Editable,
            field_type: FieldType::Float { decimals: 2 },
            constraints: Some(Constraints {
                min: Some(-90.0),
                max: Some(90.0),
                step: Some(0.01),
            }),
            help_text: Some("Latitude for sunrise/sunset calculation".into()),
        },
    );

    fields.insert(
        "longitude".into(),
        FieldSchema {
            available: true,
            state: FieldState::Editable,
            field_type: FieldType::Float { decimals: 2 },
            constraints: Some(Constraints {
                min: Some(-180.0),
                max: Some(180.0),
                step: Some(0.01),
            }),
            help_text: Some("Longitude for sunrise/sunset calculation".into()),
        },
    );

    fields.insert(
        "auto_location".into(),
        FieldSchema {
            available: true,
            state: FieldState::Editable,
            field_type: FieldType::Bool,
            constraints: None,
            help_text: Some("Auto-detect location via geoclue".into()),
        },
    );

    fields.insert(
        "dbus_api".into(),
        FieldSchema {
            available: true,
            state: FieldState::Editable,
            field_type: FieldType::Bool,
            constraints: None,
            help_text: Some("Enable D-Bus API (required for waft)".into()),
        },
    );

    fields.insert(
        "portal_api".into(),
        FieldSchema {
            available: true,
            state: FieldState::Editable,
            field_type: FieldType::Bool,
            constraints: None,
            help_text: Some("Enable XDG portal support".into()),
        },
    );

    ConfigSchema { fields }
}

/// Validate a field value against constraints.
pub fn validate_field(field: &str, value: &serde_json::Value) -> Result<()> {
    match field {
        "latitude" => {
            let lat: f64 =
                serde_json::from_value(value.clone()).context("Latitude must be a number")?;
            if lat < -90.0 || lat > 90.0 {
                return Err(anyhow::anyhow!("Latitude must be between -90 and 90"));
            }
        }
        "longitude" => {
            let lng: f64 =
                serde_json::from_value(value.clone()).context("Longitude must be a number")?;
            if lng < -180.0 || lng > 180.0 {
                return Err(anyhow::anyhow!("Longitude must be between -180 and 180"));
            }
        }
        "auto_location" | "dbus_api" | "portal_api" => {
            let _: bool =
                serde_json::from_value(value.clone()).context("Value must be a boolean")?;
        }
        _ => {
            return Err(anyhow::anyhow!("Unknown field: {}", field));
        }
    }
    Ok(())
}

/// Restart darkman service via systemctl (best-effort).
pub async fn restart_darkman_service() -> Result<()> {
    let output = tokio::process::Command::new("systemctl")
        .args(["--user", "restart", "darkman.service"])
        .output()
        .await;

    match output {
        Ok(out) if out.status.success() => {
            log::info!("[darkman-config] Service restarted successfully");
            Ok(())
        }
        Ok(out) => {
            let stderr = String::from_utf8_lossy(&out.stderr);
            Err(anyhow::anyhow!("systemctl restart failed: {}", stderr))
        }
        Err(e) => Err(anyhow::anyhow!("Failed to execute systemctl: {}", e)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_valid_config() {
        let yaml = r#"
lat: 50.08
lng: 14.42
usegeoclue: true
dbusserver: true
portal: true
"#;
        let config: DarkmanYamlConfig = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(config.lat, Some(50.08));
        assert_eq!(config.lng, Some(14.42));
        assert_eq!(config.usegeoclue, Some(true));
        assert_eq!(config.dbusserver, Some(true));
        assert_eq!(config.portal, Some(true));
    }

    #[test]
    fn parse_partial_config() {
        let yaml = r#"
lat: 52.52
lng: 13.40
"#;
        let config: DarkmanYamlConfig = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(config.lat, Some(52.52));
        assert_eq!(config.lng, Some(13.40));
        assert_eq!(config.usegeoclue, None);
        assert_eq!(config.dbusserver, None);
        assert_eq!(config.portal, None);
    }

    #[test]
    fn parse_empty_config() {
        let config = DarkmanYamlConfig::default();
        assert_eq!(config.lat, None);
        assert_eq!(config.usegeoclue, None);
    }

    #[test]
    fn schema_contains_all_fields() {
        let schema = build_schema();
        assert!(schema.fields.contains_key("latitude"));
        assert!(schema.fields.contains_key("longitude"));
        assert!(schema.fields.contains_key("auto_location"));
        assert!(schema.fields.contains_key("dbus_api"));
        assert!(schema.fields.contains_key("portal_api"));
        assert_eq!(schema.fields.len(), 5);
    }

    #[test]
    fn latitude_field_schema() {
        let schema = build_schema();
        let lat = &schema.fields["latitude"];
        assert!(lat.available);
        assert_eq!(lat.state, FieldState::Editable);
        assert_eq!(lat.field_type, FieldType::Float { decimals: 2 });
        let c = lat.constraints.as_ref().unwrap();
        assert_eq!(c.min, Some(-90.0));
        assert_eq!(c.max, Some(90.0));
        assert_eq!(c.step, Some(0.01));
    }

    #[test]
    fn validate_latitude_in_range() {
        assert!(validate_field("latitude", &serde_json::json!(52.52)).is_ok());
    }

    #[test]
    fn validate_latitude_out_of_range_high() {
        assert!(validate_field("latitude", &serde_json::json!(100.0)).is_err());
    }

    #[test]
    fn validate_latitude_out_of_range_low() {
        assert!(validate_field("latitude", &serde_json::json!(-100.0)).is_err());
    }

    #[test]
    fn validate_longitude_in_range() {
        assert!(validate_field("longitude", &serde_json::json!(14.42)).is_ok());
    }

    #[test]
    fn validate_longitude_out_of_range() {
        assert!(validate_field("longitude", &serde_json::json!(200.0)).is_err());
    }

    #[test]
    fn validate_bool_fields() {
        let value = serde_json::json!(true);
        assert!(validate_field("auto_location", &value).is_ok());
        assert!(validate_field("dbus_api", &value).is_ok());
        assert!(validate_field("portal_api", &value).is_ok());
    }

    #[test]
    fn validate_unknown_field() {
        assert!(validate_field("unknown_field", &serde_json::json!(42)).is_err());
    }
}
