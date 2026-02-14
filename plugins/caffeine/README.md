# Caffeine Plugin

Screen lock and screensaver inhibition toggle.

## Purpose

Prevents the screen from locking or the screensaver from activating when enabled. Uses the XDG Desktop Portal Inhibit interface as the primary backend, falling back to the freedesktop ScreenSaver interface. The plugin reports `can_stop = false` while actively inhibiting to prevent the daemon from shutting it down and losing the inhibition.

## Entity Types

### `sleep-inhibitor`

Single entity representing the caffeine toggle state.

| Field | Type | Description |
|-------|------|-------------|
| `active` | `bool` | Whether screen lock inhibition is currently active |

### Actions

| Action | Params | Description |
|--------|--------|-------------|
| `toggle` | none | Toggle inhibition on/off |

### URN Format

```
caffeine/sleep-inhibitor/default
```

## D-Bus Interfaces

### Primary: XDG Desktop Portal

| Bus | Destination | Path | Interface | Usage |
|-----|-------------|------|-----------|-------|
| Session | `org.freedesktop.portal.Desktop` | `/org/freedesktop/portal/desktop` | `org.freedesktop.portal.Inhibit` | Inhibit screen lock (flag 8 = idle inhibit) |

### Fallback: freedesktop ScreenSaver

| Bus | Destination | Path | Interface | Usage |
|-----|-------------|------|-----------|-------|
| Session | `org.freedesktop.ScreenSaver` | `/ScreenSaver` or `/org/freedesktop/ScreenSaver` | `org.freedesktop.ScreenSaver` | Inhibit/UnInhibit with cookie |

The plugin probes Portal first, then tries ScreenSaver paths. Fails to start if neither backend is available.

## Dependencies

- **xdg-desktop-portal** (preferred) -- portal-based inhibition
- **freedesktop ScreenSaver service** (fallback) -- cookie-based inhibition

## Configuration

```toml
[[plugins]]
id = "caffeine"
```

No plugin-specific configuration options.
