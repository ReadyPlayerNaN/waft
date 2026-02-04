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
        let key = match self {
            Self::Clear => "weather-clear",
            Self::PartlyCloudy => "weather-partly-cloudy",
            Self::Cloudy => "weather-cloudy",
            Self::Fog => "weather-fog",
            Self::Drizzle => "weather-drizzle",
            Self::Rain => "weather-rain",
            Self::FreezingRain => "weather-freezing-rain",
            Self::Snow => "weather-snow",
            Self::Thunderstorm => "weather-thunderstorm",
        };
        crate::i18n::t(key)
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
    pub fn from_str(s: &str) -> Self {
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
