use std::collections::HashMap;
use waft_protocol::entity::display::{FieldState, NightLightConfig};

/// All config field names that sunsetr exposes.
const ALL_FIELDS: &[&str] = &[
    "backend",
    "transition_mode",
    "night_temp",
    "night_gamma",
    "day_temp",
    "day_gamma",
    "static_temp",
    "static_gamma",
    "sunset",
    "sunrise",
    "transition_duration",
    "latitude",
    "longitude",
    "smoothing",
    "startup_duration",
    "shutdown_duration",
    "adaptive_interval",
    "update_interval",
];

/// Compute field states based on transition_mode.
pub fn compute_field_states(transition_mode: &str) -> HashMap<String, FieldState> {
    let mut states = HashMap::new();

    // Always editable fields
    for field in &[
        "backend",
        "transition_mode",
        "smoothing",
        "startup_duration",
        "shutdown_duration",
        "adaptive_interval",
        "update_interval",
    ] {
        states.insert((*field).to_string(), FieldState::Editable);
    }

    match transition_mode {
        "geo" => {
            states.insert("latitude".into(), FieldState::Editable);
            states.insert("longitude".into(), FieldState::Editable);
            states.insert("day_temp".into(), FieldState::Editable);
            states.insert("day_gamma".into(), FieldState::Editable);
            states.insert("night_temp".into(), FieldState::Editable);
            states.insert("night_gamma".into(), FieldState::Editable);
            states.insert("transition_duration".into(), FieldState::Editable);
            states.insert("sunrise".into(), FieldState::ReadOnly);
            states.insert("sunset".into(), FieldState::ReadOnly);
            states.insert("static_temp".into(), FieldState::Disabled);
            states.insert("static_gamma".into(), FieldState::Disabled);
        }
        "static" => {
            states.insert("static_temp".into(), FieldState::Editable);
            states.insert("static_gamma".into(), FieldState::Editable);
            states.insert("day_temp".into(), FieldState::Disabled);
            states.insert("day_gamma".into(), FieldState::Disabled);
            states.insert("night_temp".into(), FieldState::Disabled);
            states.insert("night_gamma".into(), FieldState::Disabled);
            states.insert("sunrise".into(), FieldState::Disabled);
            states.insert("sunset".into(), FieldState::Disabled);
            states.insert("latitude".into(), FieldState::Disabled);
            states.insert("longitude".into(), FieldState::Disabled);
            states.insert("transition_duration".into(), FieldState::Disabled);
        }
        // center, finish_by, start_at
        _ => {
            states.insert("sunrise".into(), FieldState::Editable);
            states.insert("sunset".into(), FieldState::Editable);
            states.insert("day_temp".into(), FieldState::Editable);
            states.insert("day_gamma".into(), FieldState::Editable);
            states.insert("night_temp".into(), FieldState::Editable);
            states.insert("night_gamma".into(), FieldState::Editable);
            states.insert("transition_duration".into(), FieldState::Editable);
            states.insert("latitude".into(), FieldState::Disabled);
            states.insert("longitude".into(), FieldState::Disabled);
            states.insert("static_temp".into(), FieldState::Disabled);
            states.insert("static_gamma".into(), FieldState::Disabled);
        }
    }

    states
}

/// Build a NightLightConfig entity from a map of sunsetr JSON key-value pairs.
pub fn build_config_entity(
    target: &str,
    values: &HashMap<String, String>,
) -> NightLightConfig {
    let transition_mode = values
        .get("transition_mode")
        .cloned()
        .unwrap_or_else(|| "geo".to_string());
    let field_state = compute_field_states(&transition_mode);

    NightLightConfig {
        target: target.to_string(),
        backend: values.get("backend").cloned().unwrap_or_default(),
        transition_mode,
        night_temp: values.get("night_temp").cloned().unwrap_or_default(),
        night_gamma: values.get("night_gamma").cloned().unwrap_or_default(),
        day_temp: values.get("day_temp").cloned().unwrap_or_default(),
        day_gamma: values.get("day_gamma").cloned().unwrap_or_default(),
        static_temp: values.get("static_temp").cloned().unwrap_or_default(),
        static_gamma: values.get("static_gamma").cloned().unwrap_or_default(),
        sunset: values.get("sunset").cloned().unwrap_or_default(),
        sunrise: values.get("sunrise").cloned().unwrap_or_default(),
        transition_duration: values
            .get("transition_duration")
            .cloned()
            .unwrap_or_default(),
        latitude: values.get("latitude").cloned().unwrap_or_default(),
        longitude: values.get("longitude").cloned().unwrap_or_default(),
        smoothing: values.get("smoothing").cloned().unwrap_or_default(),
        startup_duration: values
            .get("startup_duration")
            .cloned()
            .unwrap_or_default(),
        shutdown_duration: values
            .get("shutdown_duration")
            .cloned()
            .unwrap_or_default(),
        adaptive_interval: values
            .get("adaptive_interval")
            .cloned()
            .unwrap_or_default(),
        update_interval: values
            .get("update_interval")
            .cloned()
            .unwrap_or_default(),
        field_state,
    }
}

/// Validate a config field name is known.
pub fn validate_field_name(field: &str) -> bool {
    ALL_FIELDS.contains(&field)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn geo_mode_field_states() {
        let states = compute_field_states("geo");
        assert_eq!(states["latitude"], FieldState::Editable);
        assert_eq!(states["longitude"], FieldState::Editable);
        assert_eq!(states["sunrise"], FieldState::ReadOnly);
        assert_eq!(states["sunset"], FieldState::ReadOnly);
        assert_eq!(states["static_temp"], FieldState::Disabled);
        assert_eq!(states["static_gamma"], FieldState::Disabled);
        assert_eq!(states["day_temp"], FieldState::Editable);
        assert_eq!(states["night_temp"], FieldState::Editable);
        assert_eq!(states["transition_mode"], FieldState::Editable);
        assert_eq!(states["smoothing"], FieldState::Editable);
    }

    #[test]
    fn static_mode_field_states() {
        let states = compute_field_states("static");
        assert_eq!(states["static_temp"], FieldState::Editable);
        assert_eq!(states["static_gamma"], FieldState::Editable);
        assert_eq!(states["day_temp"], FieldState::Disabled);
        assert_eq!(states["night_temp"], FieldState::Disabled);
        assert_eq!(states["latitude"], FieldState::Disabled);
        assert_eq!(states["sunrise"], FieldState::Disabled);
    }

    #[test]
    fn center_mode_field_states() {
        let states = compute_field_states("center");
        assert_eq!(states["sunrise"], FieldState::Editable);
        assert_eq!(states["sunset"], FieldState::Editable);
        assert_eq!(states["day_temp"], FieldState::Editable);
        assert_eq!(states["latitude"], FieldState::Disabled);
        assert_eq!(states["static_temp"], FieldState::Disabled);
    }

    #[test]
    fn finish_by_mode_field_states() {
        let states = compute_field_states("finish_by");
        assert_eq!(states["sunrise"], FieldState::Editable);
        assert_eq!(states["sunset"], FieldState::Editable);
        assert_eq!(states["latitude"], FieldState::Disabled);
    }

    #[test]
    fn start_at_mode_field_states() {
        let states = compute_field_states("start_at");
        assert_eq!(states["sunrise"], FieldState::Editable);
        assert_eq!(states["sunset"], FieldState::Editable);
        assert_eq!(states["latitude"], FieldState::Disabled);
    }

    #[test]
    fn build_config_entity_from_values() {
        let mut values = HashMap::new();
        values.insert("backend".into(), "auto".into());
        values.insert("transition_mode".into(), "geo".into());
        values.insert("night_temp".into(), "3500".into());
        values.insert("night_gamma".into(), "100".into());
        values.insert("day_temp".into(), "6500".into());
        values.insert("day_gamma".into(), "100".into());
        values.insert("static_temp".into(), "4500".into());
        values.insert("static_gamma".into(), "100".into());
        values.insert("sunset".into(), "20:30:00".into());
        values.insert("sunrise".into(), "06:00:00".into());
        values.insert("transition_duration".into(), "30".into());
        values.insert("latitude".into(), "50.08".into());
        values.insert("longitude".into(), "14.42".into());
        values.insert("smoothing".into(), "true".into());
        values.insert("startup_duration".into(), "1.0".into());
        values.insert("shutdown_duration".into(), "1.0".into());
        values.insert("adaptive_interval".into(), "100".into());
        values.insert("update_interval".into(), "5".into());

        let config = build_config_entity("default", &values);

        assert_eq!(config.target, "default");
        assert_eq!(config.backend, "auto");
        assert_eq!(config.transition_mode, "geo");
        assert_eq!(config.night_temp, "3500");
        assert_eq!(config.latitude, "50.08");
        assert_eq!(config.field_state["latitude"], FieldState::Editable);
        assert_eq!(config.field_state["sunrise"], FieldState::ReadOnly);
        assert_eq!(config.field_state["static_temp"], FieldState::Disabled);
    }

    #[test]
    fn build_config_entity_defaults_for_missing_fields() {
        let values = HashMap::new();
        let config = build_config_entity("default", &values);

        assert_eq!(config.transition_mode, "geo");
        assert_eq!(config.backend, "");
        assert_eq!(config.night_temp, "");
    }

    #[test]
    fn validate_known_fields() {
        assert!(validate_field_name("backend"));
        assert!(validate_field_name("transition_mode"));
        assert!(validate_field_name("night_temp"));
        assert!(validate_field_name("latitude"));
        assert!(validate_field_name("smoothing"));
        assert!(!validate_field_name("unknown_field"));
    }
}
