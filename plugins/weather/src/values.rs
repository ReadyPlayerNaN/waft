//! Weather data types and WMO weather code mappings.

/// Weather condition categories derived from WMO weather codes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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

impl WeatherCondition {
    /// Map WMO weather code to a WeatherCondition.
    /// See: https://open-meteo.com/en/docs#weathervariables
    pub fn from_wmo_code(code: i32) -> Self {
        match code {
            0 => Self::Clear,
            1..=2 => Self::PartlyCloudy,
            3 => Self::Cloudy,
            45 | 48 => Self::Fog,
            51 | 53 | 55 => Self::Drizzle,
            56 | 57 => Self::FreezingRain,
            61 | 63 | 65 | 80 | 81 | 82 => Self::Rain,
            66 | 67 => Self::FreezingRain,
            71 | 73 | 75 | 77 | 85 | 86 => Self::Snow,
            95 | 96 | 99 => Self::Thunderstorm,
            _ => Self::Clear,
        }
    }

    /// Get the GTK icon name for this condition.
    pub fn icon_name(&self, is_day: bool) -> &'static str {
        match (self, is_day) {
            (Self::Clear, true) => "weather-clear-symbolic",
            (Self::Clear, false) => "weather-clear-night-symbolic",
            (Self::PartlyCloudy, true) => "weather-few-clouds-symbolic",
            (Self::PartlyCloudy, false) => "weather-few-clouds-night-symbolic",
            (Self::Cloudy, _) => "weather-overcast-symbolic",
            (Self::Fog, _) => "weather-fog-symbolic",
            (Self::Drizzle, _) => "weather-showers-scattered-symbolic",
            (Self::Rain, _) => "weather-showers-symbolic",
            (Self::FreezingRain, _) => "weather-showers-symbolic",
            (Self::Snow, _) => "weather-snow-symbolic",
            (Self::Thunderstorm, _) => "weather-storm-symbolic",
        }
    }

    /// Get a human-readable description of this condition.
    pub fn description(&self) -> String {
        let i18n = crate::i18n::i18n();
        match self {
            Self::Clear => i18n.t("weather-clear"),
            Self::PartlyCloudy => i18n.t("weather-partly-cloudy"),
            Self::Cloudy => i18n.t("weather-cloudy"),
            Self::Fog => i18n.t("weather-fog"),
            Self::Drizzle => i18n.t("weather-drizzle"),
            Self::Rain => i18n.t("weather-rain"),
            Self::FreezingRain => i18n.t("weather-freezing-rain"),
            Self::Snow => i18n.t("weather-snow"),
            Self::Thunderstorm => i18n.t("weather-thunderstorm"),
        }
    }
}

/// Current weather data from the API.
#[derive(Debug, Clone)]
pub struct WeatherData {
    pub temperature: f64,
    pub condition: WeatherCondition,
    pub is_day: bool,
}

/// Temperature unit for display.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TemperatureUnit {
    #[default]
    Celsius,
    Fahrenheit,
}

impl TemperatureUnit {
    pub fn parse(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "fahrenheit" | "f" => Self::Fahrenheit,
            _ => Self::Celsius,
        }
    }

    pub fn symbol(&self) -> &'static str {
        match self {
            Self::Celsius => "C",
            Self::Fahrenheit => "F",
        }
    }

    pub fn api_value(&self) -> &'static str {
        match self {
            Self::Celsius => "celsius",
            Self::Fahrenheit => "fahrenheit",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // WeatherCondition::from_wmo_code tests
    #[test]
    fn test_wmo_code_0_is_clear() {
        assert_eq!(WeatherCondition::from_wmo_code(0), WeatherCondition::Clear);
    }

    #[test]
    fn test_wmo_codes_1_2_are_partly_cloudy() {
        assert_eq!(WeatherCondition::from_wmo_code(1), WeatherCondition::PartlyCloudy);
        assert_eq!(WeatherCondition::from_wmo_code(2), WeatherCondition::PartlyCloudy);
    }

    #[test]
    fn test_wmo_code_3_is_cloudy() {
        assert_eq!(WeatherCondition::from_wmo_code(3), WeatherCondition::Cloudy);
    }

    #[test]
    fn test_wmo_codes_fog() {
        assert_eq!(WeatherCondition::from_wmo_code(45), WeatherCondition::Fog);
        assert_eq!(WeatherCondition::from_wmo_code(48), WeatherCondition::Fog);
    }

    #[test]
    fn test_wmo_codes_drizzle() {
        assert_eq!(WeatherCondition::from_wmo_code(51), WeatherCondition::Drizzle);
        assert_eq!(WeatherCondition::from_wmo_code(53), WeatherCondition::Drizzle);
        assert_eq!(WeatherCondition::from_wmo_code(55), WeatherCondition::Drizzle);
    }

    #[test]
    fn test_wmo_codes_freezing_rain() {
        assert_eq!(WeatherCondition::from_wmo_code(56), WeatherCondition::FreezingRain);
        assert_eq!(WeatherCondition::from_wmo_code(57), WeatherCondition::FreezingRain);
        assert_eq!(WeatherCondition::from_wmo_code(66), WeatherCondition::FreezingRain);
        assert_eq!(WeatherCondition::from_wmo_code(67), WeatherCondition::FreezingRain);
    }

    #[test]
    fn test_wmo_codes_rain() {
        assert_eq!(WeatherCondition::from_wmo_code(61), WeatherCondition::Rain);
        assert_eq!(WeatherCondition::from_wmo_code(63), WeatherCondition::Rain);
        assert_eq!(WeatherCondition::from_wmo_code(65), WeatherCondition::Rain);
        assert_eq!(WeatherCondition::from_wmo_code(80), WeatherCondition::Rain);
        assert_eq!(WeatherCondition::from_wmo_code(81), WeatherCondition::Rain);
        assert_eq!(WeatherCondition::from_wmo_code(82), WeatherCondition::Rain);
    }

    #[test]
    fn test_wmo_codes_snow() {
        assert_eq!(WeatherCondition::from_wmo_code(71), WeatherCondition::Snow);
        assert_eq!(WeatherCondition::from_wmo_code(73), WeatherCondition::Snow);
        assert_eq!(WeatherCondition::from_wmo_code(75), WeatherCondition::Snow);
        assert_eq!(WeatherCondition::from_wmo_code(77), WeatherCondition::Snow);
        assert_eq!(WeatherCondition::from_wmo_code(85), WeatherCondition::Snow);
        assert_eq!(WeatherCondition::from_wmo_code(86), WeatherCondition::Snow);
    }

    #[test]
    fn test_wmo_codes_thunderstorm() {
        assert_eq!(WeatherCondition::from_wmo_code(95), WeatherCondition::Thunderstorm);
        assert_eq!(WeatherCondition::from_wmo_code(96), WeatherCondition::Thunderstorm);
        assert_eq!(WeatherCondition::from_wmo_code(99), WeatherCondition::Thunderstorm);
    }

    #[test]
    fn test_wmo_code_unknown_defaults_to_clear() {
        assert_eq!(WeatherCondition::from_wmo_code(100), WeatherCondition::Clear);
        assert_eq!(WeatherCondition::from_wmo_code(-1), WeatherCondition::Clear);
        assert_eq!(WeatherCondition::from_wmo_code(999), WeatherCondition::Clear);
    }

    // WeatherCondition::icon_name tests
    #[test]
    fn test_clear_icon_day_vs_night() {
        assert_eq!(
            WeatherCondition::Clear.icon_name(true),
            "weather-clear-symbolic"
        );
        assert_eq!(
            WeatherCondition::Clear.icon_name(false),
            "weather-clear-night-symbolic"
        );
    }

    #[test]
    fn test_partly_cloudy_icon_day_vs_night() {
        assert_eq!(
            WeatherCondition::PartlyCloudy.icon_name(true),
            "weather-few-clouds-symbolic"
        );
        assert_eq!(
            WeatherCondition::PartlyCloudy.icon_name(false),
            "weather-few-clouds-night-symbolic"
        );
    }

    #[test]
    fn test_cloudy_icon_same_day_and_night() {
        assert_eq!(
            WeatherCondition::Cloudy.icon_name(true),
            "weather-overcast-symbolic"
        );
        assert_eq!(
            WeatherCondition::Cloudy.icon_name(false),
            "weather-overcast-symbolic"
        );
    }

    #[test]
    fn test_fog_icon() {
        assert_eq!(WeatherCondition::Fog.icon_name(true), "weather-fog-symbolic");
    }

    #[test]
    fn test_drizzle_icon() {
        assert_eq!(
            WeatherCondition::Drizzle.icon_name(true),
            "weather-showers-scattered-symbolic"
        );
    }

    #[test]
    fn test_rain_icon() {
        assert_eq!(
            WeatherCondition::Rain.icon_name(true),
            "weather-showers-symbolic"
        );
    }

    #[test]
    fn test_snow_icon() {
        assert_eq!(WeatherCondition::Snow.icon_name(true), "weather-snow-symbolic");
    }

    #[test]
    fn test_thunderstorm_icon() {
        assert_eq!(
            WeatherCondition::Thunderstorm.icon_name(true),
            "weather-storm-symbolic"
        );
    }

    // TemperatureUnit tests
    #[test]
    fn test_temperature_unit_from_str_celsius() {
        assert_eq!(TemperatureUnit::parse("celsius"), TemperatureUnit::Celsius);
        assert_eq!(TemperatureUnit::parse("Celsius"), TemperatureUnit::Celsius);
        assert_eq!(TemperatureUnit::parse("CELSIUS"), TemperatureUnit::Celsius);
        assert_eq!(TemperatureUnit::parse("c"), TemperatureUnit::Celsius);
    }

    #[test]
    fn test_temperature_unit_from_str_fahrenheit() {
        assert_eq!(TemperatureUnit::parse("fahrenheit"), TemperatureUnit::Fahrenheit);
        assert_eq!(TemperatureUnit::parse("Fahrenheit"), TemperatureUnit::Fahrenheit);
        assert_eq!(TemperatureUnit::parse("FAHRENHEIT"), TemperatureUnit::Fahrenheit);
        assert_eq!(TemperatureUnit::parse("f"), TemperatureUnit::Fahrenheit);
        assert_eq!(TemperatureUnit::parse("F"), TemperatureUnit::Fahrenheit);
    }

    #[test]
    fn test_temperature_unit_from_str_defaults_to_celsius() {
        assert_eq!(TemperatureUnit::parse("invalid"), TemperatureUnit::Celsius);
        assert_eq!(TemperatureUnit::parse(""), TemperatureUnit::Celsius);
        assert_eq!(TemperatureUnit::parse("kelvin"), TemperatureUnit::Celsius);
    }

    #[test]
    fn test_temperature_unit_default_is_celsius() {
        assert_eq!(TemperatureUnit::default(), TemperatureUnit::Celsius);
    }

    #[test]
    fn test_temperature_unit_symbol() {
        assert_eq!(TemperatureUnit::Celsius.symbol(), "C");
        assert_eq!(TemperatureUnit::Fahrenheit.symbol(), "F");
    }

    #[test]
    fn test_temperature_unit_api_value() {
        assert_eq!(TemperatureUnit::Celsius.api_value(), "celsius");
        assert_eq!(TemperatureUnit::Fahrenheit.api_value(), "fahrenheit");
    }
}
