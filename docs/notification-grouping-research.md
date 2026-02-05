# Notification Auxiliary Group Splits: Research

**Investigation Date**: 2026-02-05
**Status**: Research Complete - Pending real-world data collection

---

## Table of Contents

1. [Problem Statement](#problem-statement)
2. [Freedesktop Notifications Spec](#freedesktop-notifications-spec)
3. [What Apps Actually Send](#what-apps-actually-send)
4. [Community Efforts](#community-efforts)
5. [Available Signals for Sub-Grouping](#available-signals-for-sub-grouping)
6. [Open Questions](#open-questions)
7. [References](#references)

---

## Problem Statement

Some apps manage multiple workspaces or conversations (Slack, Discord, Telegram). Currently, all notifications from the same app are grouped into a single group based on `app_name` or `desktop-entry`. It would be useful to split notifications into sub-groups per workspace or channel so users can manage them independently.

The workspace name (if detected) should appear in the notification group header. Optionally, a workspace icon could be loaded and displayed as a secondary icon.

---

## Freedesktop Notifications Spec

The current spec (v1.3, August 2024) provides **no native grouping, threading, or sub-categorization mechanism**.

### Notify() Parameters

| Parameter        | Type        | Relevance to grouping                         |
| ---------------- | ----------- | --------------------------------------------- |
| `app_name`       | STRING      | Primary grouping key (e.g. "Slack")           |
| `replaces_id`    | UINT32      | Replaces a single notification, not grouping  |
| `app_icon`       | STRING      | Per-notification icon, not group-level         |
| `summary`        | STRING      | **Contains workspace/channel info as text**   |
| `body`           | STRING      | Message content, sometimes has context         |
| `actions`        | ARRAY       | Not relevant                                  |
| `hints`          | DICT        | See below                                     |
| `expire_timeout` | INT32       | Not relevant                                  |

### Standard Hints

| Hint              | Type    | Relevance                                              |
| ----------------- | ------- | ------------------------------------------------------ |
| `desktop-entry`   | STRING  | App identifier (e.g. "Slack", "discord")               |
| `category`        | STRING  | Semantic type like `im.received` — apps rarely set it  |
| `urgency`         | BYTE    | 0=Low, 1=Normal, 2=Critical                            |
| `action-icons`    | BOOLEAN | Not relevant                                           |
| `image-data`      | bytes   | Raw image, not relevant                                |
| `image-path`      | STRING  | Not relevant                                           |
| `resident`        | BOOLEAN | Not relevant                                           |
| `sound-file`      | STRING  | Not relevant                                           |
| `sound-name`      | STRING  | Not relevant                                           |
| `suppress-sound`  | BOOLEAN | Not relevant                                           |
| `transient`       | BOOLEAN | Not relevant                                           |
| `x`, `y`          | INT32   | Not relevant                                           |

There is **no standard hint** for workspace, channel, thread, or conversation identity.

The spec allows vendor extensions via `x-vendor-*` naming, but Electron apps do not use any relevant ones.

### Standard Categories

The spec defines categories in `class.specific` form: `call.*`, `device.*`, `email.*`, `im.*`, `network.*`, `presence.*`, `transfer.*`. Servers may use categories to group similar types, but this is coarse (all IM notifications would group together regardless of source).

---

## What Apps Actually Send

### Electron Framework (Slack, Discord, etc.)

Electron's Linux notification path (`libnotify_notification.cc`) sends:

- `desktop-entry` — from `GetXdgAppId()`, matches the `.desktop` file
- `urgency` — mapped from Electron's urgency levels
- `sender-pid` — process ID
- `x-canonical-append` — set to `"true"` if the daemon supports it

Only `title` and `body` from the Electron Notification API reach D-Bus. No workspace, channel, or threading hints are sent.

### Slack

Captured via dunst debug output:

```
app_name:       "Slack"
summary:        "[WorkspaceName] from Username"   (multi-workspace)
                "Username"                        (single workspace, DM)
                "#channel"                        (single workspace, channel)
body:           "Message text content"
icon:           "dialog-information"
desktop_entry:  "Slack"
category:       (empty)
urgency:        NORMAL (1)
expire_timeout: 10000
actions:        {"default": "View"}
```

Key observations:
- Multi-workspace Slack prefixes the summary with `[WorkspaceName]`
- The `category` hint is **always empty** — Slack does not set `im.received`
- The icon is generic `dialog-information`, not a Slack or workspace icon
- No vendor-specific hints for workspace or channel identity

### Discord

- `app_name`: `"Discord"`
- Similar minimal hint set to Slack (Electron baseline)
- No workspace/server/channel hints

### Telegram Desktop

- `app_name`: `"Telegram Desktop"`
- `summary`: chat or group name
- `body`: message content
- Native app (not Electron), but still no extra hints for grouping

---

## Community Efforts

### XDG Desktop Portal Notification v2

The xdg-desktop-portal project shipped a Notification v2 spec adding:
- `category` — semantic types like `im.received`, `call.incoming`
- `display-hint` — transient/persistent/tray
- `priority` — low/normal/high/urgent
- `markup-body`, `sound`, button purposes

**Not shipped** (deferred to hypothetical v3):
- `groupingId` — a flat string to group notifications within an app
- Threading — grouping by message thread

These were discussed in the v2 proposal (GitHub issue #983) and the future features discussion (#1495) but did not make the cut.

### GNOME Shell

GNOME's roadmap:
- **GNOME 46** (shipped): refactoring, headers, expanded drawer, larger icons
- **GNOME 47**: notification sounds, markup support
- **GNOME 48+**: threading (grouping by message thread) — prototype only

No Android-style notification channels are planned. GNOME's concept is: group by app, then optionally by thread.

### Vendor Hints

Some notification daemons invented stacking hints:
- `x-dunst-stack-tag` — replaces notifications with the same tag (dunst)
- `x-canonical-private-synchronous` — similar stacking (Ubuntu/NotifyOSD)

These are **replacement/stacking** mechanisms (newer replaces older), not grouping mechanisms.

---

## Available Signals for Sub-Grouping

Given the spec limitations, the only realistic signals for sub-grouping are:

| Signal                      | Source      | Reliability | Notes                                    |
| --------------------------- | ----------- | ----------- | ---------------------------------------- |
| `[Prefix]` in summary      | Slack       | Medium      | Only present with multiple workspaces    |
| Chat/group name in summary  | Telegram    | Medium      | Summary is the conversation name         |
| `category` hint             | Spec        | Low         | Almost never set by Electron apps        |
| `x-dunst-stack-tag`        | Vendor hint | Low         | Stacking, not grouping; dunst-specific   |

The `[Prefix]` pattern in the summary is the strongest available signal for Slack workspace splitting.

---

## Open Questions

1. **What exactly does Slack send with multiple workspaces?** The `[WorkspaceName]` prefix in summary is documented in dunst community reports but we need to verify the exact format with our own debug logging.

2. **What do other multi-context apps send?** We added debug logging to capture raw payloads from all apps. Data collection is in progress.

3. **Is summary parsing robust enough?** The `[prefix]` convention could appear in unrelated notifications. We may need to scope parsing rules per `app_name`.

4. **Should sub-grouping be generic or per-app?** Generic `[prefix]` detection is simple but could misfire. Per-app rules are safer but require maintenance.

---

## References

- [Desktop Notifications Specification (v1.3)](https://specifications.freedesktop.org/notification/latest-single/)
- [Notifications Portal v2 Proposal (xdg-desktop-portal #983)](https://github.com/flatpak/xdg-desktop-portal/issues/983)
- [Future Notification Features Discussion (#1495)](https://github.com/flatpak/xdg-desktop-portal/discussions/1495)
- [GNOME Notifications 46 and Beyond](https://blogs.gnome.org/shell-dev/2024/04/23/notifications-46-and-beyond/)
- [Electron libnotify_notification.cc](https://github.com/electron/electron/blob/main/shell/browser/notifications/linux/libnotify_notification.cc)
- [Electron PR #11957 — Fix Linux notification name and desktop-entry](https://github.com/electron/electron/pull/11957/files)
- [Dunst Issue #905 — Slack notification parameters](https://github.com/dunst-project/dunst/issues/905)
- [Slack urgency workaround gist](https://gist.github.com/andreycizov/738f80a16c9e401d6a9e77b863e67066)
