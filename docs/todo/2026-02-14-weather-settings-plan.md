# Weather Settings Page Implementation Plan

**Date:** 2026-02-14
**Status:** Implemented

## Overview

The weather settings page will be added to waft-settings, allowing users to configure their weather location via city search, choose temperature units, and set update frequency. The page subscribes to the `weather` entity from the weather plugin and displays a live preview. All configuration changes are sent as actions to the weather plugin, which updates its config file and immediately notifies subscribers with refreshed data.

**Key components:**
- Settings page UI with city search, units dropdown, and update interval dropdown
- Open-Meteo Geocoding API integration for city → lat/lon conversion
- Weather plugin gains `update-config` action handler
- Extended `WeatherConfig` with `location_name` field
- Live weather preview matching the overview display

## UI Components

The Weather settings page follows the smart container + dumb widgets pattern used in Bluetooth/WiFi pages.

### WeatherPage (smart container)

- Subscribes to `weather` entity type via `EntityStore`
- Owns action callback for triggering config updates
- Contains: location settings group, weather preview group

### LocationSettingsGroup (dumb widget)

- City search entry with autocomplete dropdown (using `gtk::SearchEntry` + `gtk::ListBox`)
- Manual lat/lon entries (disabled when city is selected, enabled for custom coordinates)
- Units dropdown (`adw::ComboRow` with "Celsius" / "Fahrenheit")
- Update interval dropdown (`adw::ComboRow` with "5 min", "10 min", "30 min", "1 hour")
- Emits `LocationSettingsOutput::ConfigChanged { city, lat, lon, units, interval }`

### WeatherPreviewGroup (dumb widget)

- Shows current weather icon, temperature, condition text
- Props: `temperature`, `condition`, `day`, `location_name`
- Displays loading state while waiting for first entity
- Hidden until entity arrives (matches pattern from other features)

### Sidebar addition

- Add "Weather" row with `weather-symbolic` icon between "Wired" and "Display"

## Data Flow

### Settings → Plugin (write path)

1. User searches for a city → `LocationSettingsGroup` calls Open-Meteo Geocoding API
2. User selects a result → widget emits `ConfigChanged` output event
3. `WeatherPage` receives event, calls `action_callback` with URN `weather/weather/default`, action `update-config`, params:
   ```json
   {
     "location_name": "Prague, Czechia",
     "latitude": 50.0755,
     "longitude": 14.4378,
     "units": "celsius",
     "update_interval": 600
   }
   ```
4. Weather plugin receives action, validates params, writes to config file
5. Plugin immediately fetches fresh weather data and calls `notifier.notify()`

### Plugin → Settings (read path)

1. Plugin sends `EntityUpdated` with new weather entity
2. Daemon routes to waft-settings
3. `WeatherPage` receives entity update via subscription callback
4. Updates `WeatherPreviewGroup` props with current temperature/condition
5. Updates `LocationSettingsGroup` to show current config values

### Initial load

- Settings page subscribes to `weather` entity type
- Daemon requests status from weather plugin
- Plugin returns current entity (or empty if no data yet)
- Preview shows loading state until first entity arrives

## Weather Plugin Changes

### New action: `update-config`

The weather plugin's `handle_action` method will support:

```rust
async fn handle_action(
    &self,
    urn: Urn,
    action: String,
    params: serde_json::Value,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    match action.as_str() {
        "update-config" => {
            let new_config: WeatherConfig = serde_json::from_value(params)?;

            // Validate
            if new_config.latitude < -90.0 || new_config.latitude > 90.0 {
                return Err("Invalid latitude".into());
            }
            if new_config.longitude < -180.0 || new_config.longitude > 180.0 {
                return Err("Invalid longitude".into());
            }

            // Write to config file
            waft_plugin::config::save_plugin_config("weather", &new_config)?;

            // Update plugin state and fetch immediately
            *self.config.lock().unwrap() = new_config.clone();
            let weather_data = fetch_weather(new_config.latitude, new_config.longitude,
                                            TemperatureUnit::parse(&new_config.units)).await?;
            *self.state.lock().unwrap() = Some(Ok(weather_data));

            // Notify subscribers
            notifier.notify();

            Ok(())
        }
        _ => Err(format!("Unknown action: {}", action).into())
    }
}
```

### Plugin state changes

- Add `config: Arc<StdMutex<WeatherConfig>>` field to `WeatherPlugin`
- Periodic fetch task reads from `config` instead of using hardcoded values
- Config changes trigger immediate fetch (no waiting for next interval)

### New config helper

- Add `waft_plugin::config::save_plugin_config(plugin_id, config)` to write config back to file
- Updates the plugin's entry in `~/.config/waft/config.toml`

## Config Structure

### Updated WeatherConfig

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct WeatherConfig {
    /// Human-readable location name (e.g., "Prague, Czechia")
    pub location_name: Option<String>,
    /// Latitude for weather location
    pub latitude: f64,
    /// Longitude for weather location
    pub longitude: f64,
    /// Temperature units: "celsius" or "fahrenheit"
    pub units: String,
    /// Update interval in seconds
    pub update_interval: u64,
}

impl Default for WeatherConfig {
    fn default() -> Self {
        Self {
            location_name: Some("Prague, Czechia".to_string()),
            latitude: 50.0755,
            longitude: 14.4378,
            units: "celsius".to_string(),
            update_interval: 600,
        }
    }
}
```

### TOML representation

```toml
[[plugins]]
id = "weather"
location_name = "Prague, Czechia"
latitude = 50.0755
longitude = 14.4378
units = "celsius"
update_interval = 600
```

### Backward compatibility

- Existing configs without `location_name` work fine (`Option<String>` defaults to `None`)
- Display shows coordinates if `location_name` is missing

## Geocoding Integration

### Open-Meteo Geocoding API

- Endpoint: `https://geocoding-api.open-meteo.com/v1/search?name={query}&count=5&language=en&format=json`
- Returns: Array of locations with `name`, `country`, `latitude`, `longitude`, `admin1` (state/region)
- No API key required

### Implementation approach

- New module: `crates/settings/src/weather/geocoding.rs`
- Async function: `search_cities(query: &str) -> Result<Vec<GeocodingResult>>`
- Debounced search (300ms delay after last keystroke)
- Minimum 2 characters before searching

### GeocodingResult struct

```rust
struct GeocodingResult {
    display_name: String,  // "Prague, Czechia"
    latitude: f64,
    longitude: f64,
}
```

### UI flow

1. User types in search entry
2. After 300ms debounce, call `search_cities()`
3. Show results in dropdown (`gtk::Popover` with `gtk::ListBox`)
4. User clicks result → populate lat/lon fields, emit `ConfigChanged`
5. Handle errors (no results, network failure) with inline messages

### Caching

- No caching initially (searches are fast, < 100ms typically)
- Can add later if needed

## Error Handling

### Geocoding errors

- Network failure → Show inline error below search entry: "Unable to search. Check network connection."
- No results → Show "No locations found for '{query}'"
- API rate limit (unlikely with Open-Meteo) → Show "Search temporarily unavailable. Try again shortly."

### Config update errors

- Action fails (invalid lat/lon, file write error) → Show toast notification with error message
- Network failure during immediate weather fetch → Config still saved, preview shows previous data with "Updating..." message
- Plugin not running → Action times out, show toast: "Weather service unavailable"

### Preview display

- No entity yet → Show loading spinner with "Loading weather..."
- Entity fetch error → Show icon with "Weather data unavailable"
- Stale entity (plugin crashed) → Show last known data with warning icon

### Validation

- Latitude: -90 to 90 (validated in plugin's action handler)
- Longitude: -180 to 180 (validated in plugin's action handler)
- Update interval: Dropdown only shows valid presets (300, 600, 1800, 3600 seconds)
- Units: Dropdown only shows valid options (celsius, fahrenheit)

### Graceful degradation

- If geocoding fails, users can still manually enter coordinates
- If weather fetch fails after config update, settings are still saved
- Preview hides on error instead of showing broken state

## Testing Strategy

### Unit tests

- `WeatherConfig` serialization/deserialization with and without `location_name`
- Geocoding API response parsing (mock HTTP responses)
- Config validation (invalid lat/lon ranges)
- Action parameter validation in weather plugin

### Integration tests

- Settings page creates weather page correctly
- Geocoding search returns results for known cities
- Config update action writes to file and triggers entity update
- Preview updates when entity changes
- Sidebar navigation to weather page works

### Manual testing checklist

1. **City search flow:**
   - Type "Prague" → see results
   - Select result → lat/lon fields populate
   - Change units → preview updates
   - Change interval → config saves
   - Restart waft-settings → settings persist

2. **Manual coordinate entry:**
   - Clear city search
   - Enter custom lat/lon
   - Save → preview shows weather for custom location

3. **Error cases:**
   - Disconnect network → search shows error
   - Enter invalid coordinates → action fails gracefully
   - Kill weather plugin → preview shows "unavailable"

4. **Edge cases:**
   - Migrate existing config without `location_name` → shows coordinates
   - Very long city names → UI doesn't overflow
   - Search with special characters → doesn't crash

### Smoke test

```bash
# Start daemon
cargo run --bin waft

# Start settings
cargo run --bin waft-settings

# Navigate to Weather page
# Search for a city, change settings, verify preview updates
```

## Implementation Notes

- Follow the smart container + dumb widgets pattern from Bluetooth/WiFi pages
- Use `waft_ui_gtk::widgets::IconWidget` for weather icons (forbidden to use `gtk::Image` directly)
- Add weather page module: `crates/settings/src/pages/weather.rs`
- Add weather widgets: `crates/settings/src/weather/location_settings_group.rs`, `weather_preview_group.rs`
- Add geocoding module: `crates/settings/src/weather/geocoding.rs`
- Update sidebar: Add weather row in `crates/settings/src/sidebar.rs`
- Update window: Add weather case in `crates/settings/src/window.rs` stack switching
- Update weather plugin README.md with new action and config field documentation

## Implementation Checklist

- [x] Add `location_name: Option<String>` to `WeatherConfig` in `plugins/weather/src/lib.rs`
- [x] Add `Serialize` derive to `WeatherConfig`
- [ ] Implement `waft_plugin::config::save_plugin_config()` helper (deferred — config updates are in-memory only for now)
- [x] Add `config: Arc<StdMutex<WeatherConfig>>` field to `WeatherPlugin`
- [x] Implement `update-config` action in weather plugin's `handle_action()`
- [x] Update periodic fetch task to read from `config` field (uses `tokio::select!` with `Notify` for wake-on-change)
- [x] Create `crates/settings/src/weather/` module directory
- [x] Implement `geocoding.rs` with `search_cities()` function
- [x] Implement `location_settings_group.rs` dumb widget
- [x] Implement `weather_preview_group.rs` dumb widget
- [x] Create `crates/settings/src/pages/weather.rs` smart container
- [x] Add Weather row to sidebar in `crates/settings/src/sidebar.rs`
- [x] Add Weather page to stack in `crates/settings/src/window.rs`
- [ ] Update weather plugin README.md with action and config documentation
- [ ] Write unit tests for `WeatherConfig` serialization
- [ ] Write tests for geocoding API response parsing
- [ ] Manual testing with checklist above
