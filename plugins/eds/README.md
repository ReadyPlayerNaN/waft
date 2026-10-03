# EDS Plugin

Read-only calendar integration with Evolution Data Server. The query window runs from today's local midnight through now + 60 days. GOA authenticates accounts and EDS bridges them into calendar sources; Waft never retrieves passwords/tokens or restarts either service.

## Entity Types

| Entity | URN | Meaning |
|---|---|---|
| `calendar-event` | `eds/calendar-event/v2::{source}::{uid}::{start_time}` | Source-qualified occurrence, including defaulted `source_uid` |
| `calendar-sync` | `eds/calendar-sync/singleton` | Source-discovery availability and background refresh request outcome |
| `calendar-source-status` | `eds/calendar-source-status/{source}` | Source lifecycle, observed online state, timestamps, and sanitized diagnostics |

Identifier encoding escapes every UTF-8 byte except ASCII letters/digits/`-`/`_` as uppercase `%HH`. Identical UID/time pairs from different calendars remain independent. Legacy event IDs are not also published.

Events include summary, start/end timestamps, all-day state, description, location, and attendees. Existing recurrence support is a limited subset (daily/weekly/monthly/yearly, interval/count/until/byday/exdate); this is not full RFC 5545 recurrence conformance.

## Actions and Freshness

`refresh` on the sync singleton enqueues background work and returns immediately with `queued`, `debounced`, or `no-backends`. An unavailable/unsupported registry returns a structured error. Requests for the same queued/in-flight source merge. At most four backend requests run concurrently, each with a 30-second deadline.

`syncing` means request dispatch is active. `last_refresh` means **last refresh request**, not last remote synchronization. `Accepted` means the backend acknowledged Refresh; a successful view `Complete` means that view delivered its snapshot. Neither establishes completed remote synchronization.

Source timestamps separately identify view snapshots, refresh attempts/acknowledgments, and event delivery. `online` is the observed `Calendar.Online` boolean, or unknown; offline is not automatically expired OAuth. Credential/certificate/backend diagnostics contain no certificates, tokens, or raw event data.

## Recovery and Resource Ownership

The Tokio supervisor subscribes before source discovery and monitors registry, factory, and dynamic backend owners. Failed discovery retains stale cache. Recovery uses 1, 2, 4, 8, 16, then 30-second backoff and wakes on dependency/source changes; no service restart is performed.

Calendar eligibility includes the source and every ancestor's `Enabled`, plus ancestor collection `CalendarEnabled`. Missing parents remain Pending; invalid ancestry/data are diagnosed. Disabled/deleted sources retire their events and views. GOA account IDs are diagnostic associations only, inherited from source ancestry.

Source states are Pending, Opening, Loading, Watching, RetryWait, Disabled, Unavailable, Unsupported, or Unknown. Failed setup is retryable rather than permanently “known.” Initial and replacement views must Complete before their source-local event membership is published. Live add/modify/remove signals invalidate that source only. The old snapshot remains visible during replacement; at most one active plus one candidate view is owned per source. Retired views are stopped/disposed and shared backends are closed after their final lease.

## D-Bus Interfaces

| Session service | Path | Usage |
|---|---|---|
| `org.gnome.evolution.dataserver.Sources5` | `/org/gnome/evolution/dataserver/SourceManager` | ObjectManager source snapshots |
| `org.gnome.evolution.dataserver.Calendar8` | `/org/gnome/evolution/dataserver/CalendarFactory` | OpenCalendar |
| dynamic backend owner | dynamic calendar/view paths | Open, Online/Capabilities, GetView, Start/Stop/Dispose, Refresh, Close |

Required API absence is reported; there are no guessed service-version fallbacks. Pinned wire fixtures live in `tests/fixtures`.

## Configuration

```toml
[[plugins]]
id = "eds"
refresh_interval_secs = 480          # 8 minutes; 0 disables periodic requests
locked_refresh_interval_secs = 0    # 0 pauses while locked; nonzero requests actually run

debounce_base_secs = 15             # windows [base, 2×base, 4×base], limits [1, 2, 3]
```

The SDK also recognizes `waft::eds`; use `eds`, not the obsolete documented `plugin::eds` spelling. Invalid configuration is logged before using defaults. Interval values are bounded to `u32::MAX` seconds for timer safety; debounce arithmetic is overflow-safe.

The logind monitor resolves the session with GetSession/GetSessionByPID, reads initial LockedHint, and observes property changes. Unlock merges one immediate request. Unavailable lock monitoring logs a diagnostic and falls back to the active interval. Setup/refresh work prevents stopping; actual refresh dispatch retains the existing 120-second resource grace, not a freshness guarantee.

## Tests

`cargo test -p waft-plugin-eds` runs parser, source-policy, refresh, and private-bus lifecycle tests. `dbus-daemon` is required. Fixtures use disposable Unix sockets and never change process-global session-bus environment or access live accounts.
