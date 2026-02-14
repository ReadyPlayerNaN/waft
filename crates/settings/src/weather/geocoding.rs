//! Open-Meteo Geocoding API client.
//!
//! Searches for cities by name and returns latitude/longitude coordinates.

use serde::Deserialize;

const GEOCODING_API: &str = "https://geocoding-api.open-meteo.com/v1/search";

/// A single geocoding result.
pub struct GeocodingResult {
    pub display_name: String,
    pub latitude: f64,
    pub longitude: f64,
}

#[derive(Deserialize)]
struct ApiResponse {
    results: Option<Vec<ApiResult>>,
}

#[derive(Deserialize)]
struct ApiResult {
    name: String,
    country: Option<String>,
    admin1: Option<String>,
    latitude: f64,
    longitude: f64,
}

/// Search for cities matching the given query.
///
/// Returns up to 5 results with display names and coordinates.
/// Uses the Open-Meteo Geocoding API (no API key required).
pub async fn search_cities(query: &str) -> Result<Vec<GeocodingResult>, String> {
    let encoded = urlencoding::encode(query);
    let url = format!("{GEOCODING_API}?name={encoded}&count=5&language=en&format=json");

    let response = reqwest::get(&url)
        .await
        .map_err(|e| format!("Network error: {e}"))?;

    let api_response: ApiResponse = response
        .json()
        .await
        .map_err(|e| format!("Parse error: {e}"))?;

    let results = api_response.results.unwrap_or_default();

    Ok(results
        .into_iter()
        .map(|r| {
            let display_name = match (r.admin1.as_deref(), r.country.as_deref()) {
                (Some(region), Some(country)) => format!("{}, {}, {}", r.name, region, country),
                (None, Some(country)) => format!("{}, {}", r.name, country),
                (Some(region), None) => format!("{}, {}", r.name, region),
                (None, None) => r.name,
            };
            GeocodingResult {
                display_name,
                latitude: r.latitude,
                longitude: r.longitude,
            }
        })
        .collect())
}
