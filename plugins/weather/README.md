# Weather Plugin

Displays current weather conditions using the [Open-Meteo](https://open-meteo.com/) API. Periodically fetches temperature, weather condition, and day/night status for a configured location.

## Entity Types

| Entity Type | URN Pattern | Description |
|---|---|---|
| `weather` | `weather/weather/default` | Current weather conditions |

### `weather` entity

- `temperature` - Current temperature in configured units
- `condition` - Weather condition (Clear, PartlyCloudy, Cloudy, Fog, Drizzle, Rain, FreezingRain, Snow, Thunderstorm)
- `day` - Whether it is currently daytime

Returns no entities until the first successful API response. On fetch errors, retains the last successful data.

## Actions

None. This is a display-only plugin.

## API

Uses the [Open-Meteo Forecast API](https://open-meteo.com/en/docs) with `current=temperature_2m,weather_code,is_day` parameters. Weather codes follow the WMO standard.

No API key required.

## Configuration

```toml
[[plugins]]
id = "weather"
latitude = 50.0755
longitude = 14.4378
units = "celsius"
update_interval = 600
```

| Option | Default | Description |
|---|---|---|
| `latitude` | `50.0755` | Location latitude |
| `longitude` | `14.4378` | Location longitude |
| `units` | `"celsius"` | Temperature units: `"celsius"` or `"fahrenheit"` |
| `update_interval` | `600` | Fetch interval in seconds (default: 10 minutes) |

## Localization

Weather condition descriptions are localized via Fluent (waft-i18n). Supported locales: `en-US`, `cs-CZ`.

## Dependencies

- Network access (HTTP via reqwest to `api.open-meteo.com`)
