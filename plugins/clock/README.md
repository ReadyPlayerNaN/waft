# Clock Plugin

Displays the current time and date with locale-aware formatting. Updates on minute boundaries using sleep-to-deadline (no polling).

## Entity Types

| Entity Type | URN | Description |
|---|---|---|
| `clock` | `clock/clock/default` | Current time (HH:MM) and localized date string |

## Actions

| Action | Description |
|---|---|
| `click` | Runs the configured `on_click` command (if set) |

## How It Works

The plugin formats time as `HH:MM` and date using the system locale (detected via `waft-i18n` BCP47 locale, converted to chrono's `Locale`). A background task sleeps until the next minute boundary, then notifies the daemon to re-fetch entities.

## Configuration

```toml
[[plugins]]
id = "clock"
on_click = "gnome-calendar"  # Optional: command to run when clock is clicked
```

| Option | Type | Default | Description |
|---|---|---|---|
| `on_click` | string | `""` | Shell command executed when the clock is clicked |

## Dependencies

- **chrono** with `unstable-locales` feature for locale-aware date formatting
- **waft-i18n** for system locale detection
