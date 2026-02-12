use serde::{Deserialize, Serialize};

/// Entity type identifier for weather data.
pub const ENTITY_TYPE: &str = "weather";

/// Current weather conditions.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Weather {
    pub temperature: f64,
    pub condition: WeatherCondition,
    pub day: bool,
}

/// Weather condition classification.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum WeatherCondition {
    Clear,
    PartlyCloudy,
    Cloudy,
    Fog,
    Drizzle,
    Rain,
    FreezingRain,
    Snow,
    Thunderstorm,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn serde_roundtrip() {
        let weather = Weather {
            temperature: 22.5,
            condition: WeatherCondition::PartlyCloudy,
            day: true,
        };
        let json = serde_json::to_value(&weather).unwrap();
        let decoded: Weather = serde_json::from_value(json).unwrap();
        assert_eq!(weather, decoded);
    }

    #[test]
    fn serde_roundtrip_all_conditions() {
        let conditions = [
            WeatherCondition::Clear,
            WeatherCondition::PartlyCloudy,
            WeatherCondition::Cloudy,
            WeatherCondition::Fog,
            WeatherCondition::Drizzle,
            WeatherCondition::Rain,
            WeatherCondition::FreezingRain,
            WeatherCondition::Snow,
            WeatherCondition::Thunderstorm,
        ];
        for condition in conditions {
            let json = serde_json::to_value(condition).unwrap();
            let decoded: WeatherCondition = serde_json::from_value(json).unwrap();
            assert_eq!(condition, decoded);
        }
    }
}
