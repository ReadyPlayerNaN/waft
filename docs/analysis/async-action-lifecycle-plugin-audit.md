# Async action lifecycle plugin audit

The common `PluginRuntime` now provides action admission protection, cancellation, and serialized entity publication for every plugin using the standard runner. Plugin-specific rollback hooks are required only where a handler publishes an intermediate state before awaiting external work.

| Plugin family | External operation | Common runtime cancellation | Plugin-specific intermediate-state handling | Monitor recovery |
|---|---|---:|---:|---:|
| NetworkManager | NetworkManager D-Bus / nmrs | yes | VPN, WiFi, adapter rollback hooks | NM and BlueZ monitor reconnect loops |
| BlueZ | BlueZ D-Bus connect/disconnect | yes | Bluetooth device transition rollback | reconnect loop |
| Systemd | `systemctl` async commands | yes | state is refreshed from service entities | existing monitor lifecycle |
| Audio | pactl/wpctl commands | yes | state is refreshed after action | existing monitor lifecycle |
| Brightness | brightnessctl/ddcutil commands | yes | state is refreshed after action | inotify monitor |
| Power | UPower/power-profiles D-Bus | yes | state is authoritative from refresh | system-bus monitor reconnect loop |
| Darkman | session D-Bus | yes | state refreshes from signal | session-bus monitor reconnect loop |
| GSettings | portal/session D-Bus | yes | state refreshes from signal | session-bus monitor reconnect loop |
| Awww | awww/swww commands and Darkman D-Bus | yes | state updates through applicator | Darkman monitor reconnect loop |
| GNOME Online Accounts | GOA D-Bus | yes | state refreshes from signal | session-bus monitor reconnect loop |
| Niri / keyboard layout | compositor IPC/commands | yes | state refreshes from compositor | existing event monitor |
| Notifications | notification/sound subprocesses | yes | in-memory state is authoritative | existing event lifecycle |
| Providers | HTTP quota requests | yes | failed requests preserve last known state | poll task lifecycle |
| EDS | Evolution Data Server streams | yes | generation-scoped calendar refresh | view monitor lifecycle |
| Sunsetr / Syncthing / Caffeine | CLI, systemd, or portal operations | yes | action result followed by entity refresh | existing task lifecycle |
| Internal/XDG apps | desktop/process discovery | yes | discovery snapshots are replaceable | file/process watcher lifecycle |

## Common guarantees

- Duplicate action IDs are rejected without replacing the original tracker entry.
- Repeated non-reentrant actions are rejected by entity/action key while pending.
- Waft sends `CancelAction` to plugins that advertise `action-cancellation`.
- PluginRuntime aborts the matching task and invokes `handle_action_cancelled`.
- Entity publication is serialized per plugin.
- Normal client output is non-blocking; stalled subscribers are disconnected.
- D-Bus monitor helpers stop after repeated stream errors rather than spin.

## Follow-up fault injection

A live NetworkManager fault-injection test remains useful for confirming the exact D-Bus failure that originally produced the VPN freeze. It is not required for the lifecycle guarantees above, which are covered by daemon routing, cancellation, action admission, and workspace tests.
