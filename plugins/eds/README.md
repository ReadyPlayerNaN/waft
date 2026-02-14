# EDS Plugin

Integrates with Evolution Data Server to provide calendar events. Discovers calendar sources, opens calendar views for the next 30 days, and monitors for event additions, modifications, and removals via D-Bus signals.

## Entity Types

| Entity Type | URN | Description |
|---|---|---|
| `calendar-event` | `eds/calendar-event/{uid}@{start_time}` | A calendar event occurrence |

Each entity includes: summary, start/end time (Unix timestamp), all-day flag, description, location, and attendee list with RSVP status.

## Actions

This plugin is read-only. No actions are supported.

## D-Bus Interfaces

| Bus | Service | Path | Usage |
|---|---|---|---|
| Session | `org.gnome.evolution.dataserver.Sources5` | `/org/gnome/evolution/dataserver/SourceManager` | Discover calendar sources via `ObjectManager.GetManagedObjects` |
| Session | `org.gnome.evolution.dataserver.Calendar8` | `/org/gnome/evolution/dataserver/CalendarFactory` | Open calendar backends via `CalendarFactory.OpenCalendar` |
| Session | (dynamic bus name) | (dynamic view path) | `CalendarView.Start`, `ObjectsAdded`/`ObjectsModified`/`ObjectsRemoved` signals |

## How It Works

1. Discovers calendar sources from the EDS source registry (filters for `[Calendar]` sections)
2. Opens each calendar via the CalendarFactory D-Bus interface
3. Creates a view with a 30-day time range query using EDS S-expression syntax
4. Starts the view, which begins delivering iCalendar VEVENT data via D-Bus signals
5. Parses iCalendar data (DTSTART, DTEND, SUMMARY, ATTENDEE, etc.) into structured entities
6. Handles timezone conversion (UTC, TZID, and floating time)

## Configuration

```toml
[[plugins]]
id = "eds"
```

No plugin-specific configuration options.

## Dependencies

- **Evolution Data Server** running on the session bus
- **chrono-tz** for timezone-aware datetime parsing
