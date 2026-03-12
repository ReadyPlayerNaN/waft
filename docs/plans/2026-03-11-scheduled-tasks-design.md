# Scheduled Tasks Design

**Date:** 2026-03-11

## Overview

A new "Scheduled Tasks" page in `waft-settings` that provides a guided UI for managing systemd user timers. The feature adds a `user-timer` entity type to the existing `waft-systemd-daemon` plugin. Future backends (cron, anacron) can be added as separate pages under the same "Automation" sidebar category.

---

## Scope (v1)

- **Backend:** systemd user timers only (`~/.config/systemd/user/`)
- **Interaction:** Full CRUD with guided form (no raw unit file editing)
- **Schedule types:** Calendar (`OnCalendar=`) and Relative (`OnBootSec=` / `OnUnitActiveSec=`)
- **Service options:** Command, working directory, environment variables, `After=` dependencies, restart policy, CPU quota, memory limit
- **Status:** Inline in the timer list (last run time, exit code, next scheduled run)
- **Run now:** Per-timer action to start the associated `.service` immediately for testing

---

## Architecture

No new plugin binary. The `user-timer` entity type is added to `waft-systemd-daemon` alongside its existing `session` and `user-service` types.

```
~/.config/systemd/user/*.timer
~/.config/systemd/user/*.service
         ↕ (read/write unit files)
   waft-systemd-daemon
         ↕ (systemd1.Manager D-Bus)
         ↕ (systemctl --user for enable/start/reload)
     waft daemon
         ↕ (entity protocol)
   waft-settings scheduler page
```

**Plugin responsibilities:**
- Watch `~/.config/systemd/user/` for `*.timer` file changes (inotify)
- Query `org.freedesktop.systemd1` D-Bus for live status (last trigger, next elapse, active state, last exit code)
- Subscribe to `UnitNew`/`UnitRemoved` signals and `PropertiesChanged` on timer unit objects — no polling
- Write unit file pairs atomically on Create/Update, then run `daemon-reload`
- Run `systemctl --user enable/disable/start/stop` for lifecycle actions

---

## Entity Type

```rust
// crates/protocol/src/entity/systemd/mod.rs

pub struct UserTimer {
    pub urn: String,              // "systemd/user-timer/{name}"
    pub name: String,             // unit name without .timer suffix
    pub description: String,      // [Unit] Description=
    pub enabled: bool,
    pub active: bool,             // currently running
    pub schedule: ScheduleKind,
    pub last_trigger: Option<DateTime<Utc>>,
    pub next_elapse: Option<DateTime<Utc>>,
    pub last_exit_code: Option<i32>,
    pub command: String,          // ExecStart=
    pub working_directory: Option<String>,
    pub environment: Vec<(String, String)>,
    pub after: Vec<String>,       // After= dependencies
    pub restart: RestartPolicy,
    pub cpu_quota: Option<String>,    // e.g. "50%"
    pub memory_limit: Option<String>, // e.g. "512M"
}

pub enum ScheduleKind {
    Calendar {
        spec: String,        // OnCalendar= value (e.g. "daily", "*-*-* 09:00:00")
        persistent: bool,    // Persistent= — catch up missed runs
    },
    Relative {
        on_boot_sec: Option<Duration>,
        on_startup_sec: Option<Duration>,
        on_unit_active_sec: Option<Duration>, // repeat interval
    },
}

pub enum RestartPolicy { No, OnFailure, Always }
```

### Actions

| Action | Effect |
|---|---|
| `Enable` / `Disable` | `systemctl --user enable/disable {name}.timer` |
| `Start` | `systemctl --user start {name}.service` (run now) |
| `Stop` | `systemctl --user stop {name}.service` |
| `Create(spec)` | write unit files + daemon-reload + enable |
| `Update(spec)` | overwrite unit files + daemon-reload |
| `Delete` | stop + disable + remove unit files + daemon-reload |

---

## Unit File Generation

For a timer named `my-backup`, the plugin writes two files to `~/.config/systemd/user/`:

**`my-backup.timer` (calendar variant):**
```ini
[Unit]
Description=My Backup

[Timer]
OnCalendar=daily
Persistent=true

[Install]
WantedBy=timers.target
```

**`my-backup.timer` (relative variant):**
```ini
[Unit]
Description=My Backup

[Timer]
OnBootSec=5min
OnUnitActiveSec=6h

[Install]
WantedBy=timers.target
```

**`my-backup.service`:**
```ini
[Unit]
Description=My Backup (service)
After=network.target

[Service]
Type=oneshot
ExecStart=/home/user/scripts/backup.sh
WorkingDirectory=/home/user
Environment="KEY=value"
Restart=no
CPUQuota=50%
MemoryLimit=512M
```

### Create/Update sequence

1. Validate schedule expression: `systemd-analyze calendar <spec>` or `systemd-analyze timespan <value>`
2. Write both unit files atomically (write to temp path, then `rename`)
3. `systemctl --user daemon-reload`
4. `systemctl --user enable {name}.timer`
5. Emit updated `UserTimer` entity

### Delete sequence

1. `systemctl --user stop {name}.timer {name}.service`
2. `systemctl --user disable {name}.timer`
3. Remove both unit files
4. `systemctl --user daemon-reload`
5. Emit `EntityRemoved`

---

## Settings Page UI

### Sidebar

New **"Automation"** category in `waft-settings` sidebar (below "System"), with a single **"Scheduled Tasks"** entry. Leaves room for future cron/anacron pages.

### Timer list (`crates/settings/src/pages/scheduler.rs`)

Page header: "Scheduled Tasks" title + **"Add timer"** button (top-right).

Each timer row shows:
- Name + description (or command preview if no description)
- Schedule summary: `"Daily at 09:00"` / `"Every 6h after boot"`
- Last run: relative time + colored status dot (green = exit 0, red = non-zero exit, grey = never run)
- Next run: relative time (`"in 3 hours"`)
- Enable/disable toggle
- Three-dot menu: **Run now**, **Edit**, **Delete**

### Add/Edit dialog — two tabs

**Schedule tab:**
- Name field (auto-slugified to valid unit name)
- Description field
- Schedule kind: segmented button `Calendar | Relative`
- *Calendar mode:*
  - Presets dropdown: Hourly, Daily, Weekly, Monthly, Custom
  - `OnCalendar=` text field (editable, validated on change)
  - Persistent checkbox ("catch up missed runs")
- *Relative mode:*
  - "Delay after boot" spinner (duration)
  - "Repeat every" spinner (duration, optional)

**Service tab:**
- Command field (`ExecStart=`)
- Working directory (optional, with folder picker button)
- Environment variables: key/value list with add/remove rows
- After= dependencies: chip/tag input
- Restart policy: dropdown (No / On Failure / Always)
- CPU quota field (optional, e.g. `50%`)
- Memory limit field (optional, e.g. `512M`)

---

## Error Handling

| Failure | Behavior |
|---|---|
| Unit file write fails | Action returns error; no partial state; error shown in dialog |
| `daemon-reload` fails | Action error with systemd's stderr surfaced to user |
| `systemctl start` fails (Run now) | `last_exit_code` updates; row status dot turns red |
| D-Bus unavailable | Entity goes stale (`EntityStale`); row shows "systemd unavailable" |
| Malformed `OnCalendar=` spec | `systemd-analyze calendar` runs before save; inline error in form field |
| Malformed timespan | `systemd-analyze timespan` runs before save; inline error in form field |

Validation runs `systemd-analyze` (ships with systemd, no extra dependency) before any file is written.

---

## Future Extensibility

The "Automation" sidebar category hosts additional backends independently:

- **Cron:** `user-cron-job` entity type in `waft-protocol` + new `waft-cron-daemon` plugin (reads/writes user crontab via `crontab -l` / `crontab -`) + `cron.rs` settings page
- **Anacron:** similar pattern for `~/.config/anacron/`

No forced abstraction over backends in v1 — each gets its own page and entity type. Unified only by the sidebar grouping.
