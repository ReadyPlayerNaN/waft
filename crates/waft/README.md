# waft

Central daemon for the waft desktop shell. Discovers, spawns, and supervises plugin daemons, routing entity data and actions between plugins and apps via Unix sockets.

## Usage

```bash
waft                  # Start the daemon (default)
waft daemon           # Same as above, explicit subcommand
```

The daemon listens on `$XDG_RUNTIME_DIR/waft/daemon.sock` and registers as `org.waft.Daemon` on the session D-Bus for auto-activation.

## CLI Commands

### `waft plugin ls`

List all discovered plugin binaries and the entity types they provide.

```bash
waft plugin ls          # Human-readable table
waft -j plugin ls       # JSON output
```

Discovers plugins by scanning for `waft-*-daemon` binaries in `$WAFT_DAEMON_DIR`, `./target/debug`, `./target/release`, or `/usr/bin`.

### `waft plugin describe <name>`

Show detailed information about a specific plugin, including entity types, properties, and actions.

```bash
waft plugin describe clock
waft plugin describe bluez
waft -j plugin describe audio
```

Invokes the plugin binary with `provides --describe` to obtain runtime metadata.

### `waft protocol`

List all entity types defined in the static protocol registry. Works offline (no daemon required).

```bash
waft protocol                          # List all entity types
waft protocol --domain audio           # Filter by domain
waft protocol clock                    # Show single entity type
waft protocol -v                       # Verbose: show properties and actions
waft -j protocol                       # JSON output
```

### `waft query`

Query live entity state from a running daemon. Connects to the daemon socket, requests cached entity data, and prints results. Aliased as `waft state`.

```bash
waft query                             # Query all entity types
waft query battery                     # Query specific entity type
waft state battery                     # Same (alias)
waft query audio-device --start        # Start plugin if not running, wait for entities
waft query clock --start --timeout-ms 10000  # Custom timeout
waft -j query battery                  # JSON output
```

**Flags:**

| Flag | Description |
|---|---|
| `--start` / `-s` | Subscribe to the entity type first, triggering on-demand plugin spawning. Waits for entities to arrive before querying. Required when the plugin has not been started yet. Cannot be used without specifying an entity type. |
| `--timeout-ms <ms>` | Maximum time to wait for entities when using `--start`. Default: 5000ms. Plugins may take 1-2 seconds to produce initial entities after spawning. |

**Behavior:**

- Without `--start`: queries the daemon's entity cache. If the plugin was never started, returns empty results.
- With `--start`: sends a `Subscribe` message (triggers plugin spawning), waits up to `timeout_ms` for `EntityUpdated` notifications, then queries cached state. Sends `Unsubscribe` before disconnecting.
- Entity type names are validated against the static protocol registry before connecting. Invalid types produce an error with a hint to run `waft protocol`.
- If the daemon is not running, prints an error and exits with code 1.
- Empty results exit with code 0 and print a message to stderr. JSON mode prints `[]`.

### Global Flags

| Flag | Description |
|---|---|
| `-j` / `--json` | Output in JSON format (applies to all subcommands) |

## Environment Variables

| Variable | Description |
|---|---|
| `WAFT_DAEMON_DIR` | Override plugin binary search directory |
| `XDG_RUNTIME_DIR` | Base directory for the daemon socket (`$XDG_RUNTIME_DIR/waft/daemon.sock`) |

## D-Bus

Registers as `org.waft.Daemon` on the session bus with `DoNotQueue` flag. If another instance holds the name, startup fails. Apps can activate the daemon via D-Bus if it is not already running.
