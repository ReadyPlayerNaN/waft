//! Open-Meteo API client for weather data.

use anyhow::{Context, Result};
use serde::Deserialize;

use super::values::{TemperatureUnit, WeatherCondition, WeatherData};

const API_BASE: &str = "https://api.open-meteo.com/v1/forecast";

#[derive(Debug, Deserialize)]
struct CurrentWeather {
    temperature_2m: f64,
    weather_code: i32,
    is_day: i32,
}

#[derive(Debug, Deserialize)]
struct ApiResponse {
    current: CurrentWeather,
}

/// Fetch current weather data from Open-Meteo API.
pub async fn fetch_weather(lat: f64, lon: f64, units: TemperatureUnit) -> Result<WeatherData> {
    let url = format!(
        "{}?latitude={}&longitude={}&current=temperature_2m,weather_code,is_day&temperature_unit={}",
        API_BASE,
        lat,
        lon,
        units.api_value()
    );

    let response = reqwest::get(&url)
        .await
        .context("Failed to fetch weather data")?;

    let api_response: ApiResponse = response
        .json()
        .await
        .context("Failed to parse weather response")?;

    let current = api_response.current;

    Ok(WeatherData {
        temperature: current.temperature_2m,
        condition: WeatherCondition::from_wmo_code(current.weather_code),
        is_day: current.is_day != 0,
    })
}
