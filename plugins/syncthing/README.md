# Syncthing Plugin

Toggle for the Syncthing file synchronization service. Detects whether Syncthing is installed and manages it as a systemd user service.

## Entity Types

| Entity Type | URN Pattern | Description |
|---|---|---|
| `backup-method` | `syncthing/backup-method/syncthing` | Syncthing service state |

### `backup-method` entity

- `name` - Display name ("Syncthing")
- `enabled` - Whether the syncthing user service is active
- `icon` - Icon name (`drive-harddisk-symbolic`)

Returns no entities if syncthing is not installed.

## Actions

| Action | Params | Description |
|---|---|---|
| `toggle` | - | Start or stop the syncthing user service |
| `enable` | - | Start the syncthing user service (no-op if already running) |
| `disable` | - | Stop the syncthing user service (no-op if already stopped) |

## Service Management

- **Detection**: Checks for `syncthing` binary in `$PATH` via `which`
- **Status**: `systemctl --user is-active syncthing`
- **Start**: `systemctl --user start syncthing`
- **Stop**: `systemctl --user stop syncthing`
- **External change monitoring**: Polls service state every 30 seconds to detect changes made outside the plugin (e.g., manual `systemctl` commands)

## Configuration

```toml
[[plugins]]
id = "syncthing"
```

No plugin-specific configuration options.

## Dependencies

- [Syncthing](https://syncthing.net/) installed and configured as a systemd user service
- systemd (for `systemctl --user` service management)
