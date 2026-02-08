# Error Handling and Recovery Guide

This document describes error handling strategies, error types, and recovery mechanisms in the waft IPC infrastructure.

## Design Principles

1. **Isolation**: Plugin failures should not crash overview or affect other plugins
2. **Visibility**: Errors should be logged with sufficient context for debugging
3. **Graceful Degradation**: Failed plugins show error placeholders in UI
4. **Automatic Recovery**: Transient failures trigger retry with exponential backoff

## Error Types

### Plugin SDK Errors (`waft-plugin-sdk`)

#### `ServerError`

Plugin daemon server errors.

| Variant | Description | Recovery Strategy |
|---------|-------------|-------------------|
| `Io(std::io::Error)` | Socket I/O failure | Check socket permissions, ensure runtime dir exists |
| `Json(serde_json::Error)` | Message serialization failure | Check message format, update protocol version |
| `FrameTooLarge(usize)` | Message exceeds 10MB limit | Reduce widget complexity or split into multiple messages |
| `Other(String)` | General error (e.g., action handler failure) | Check plugin-specific logs for details |

**Common Scenarios:**

- **Socket bind failure**: Usually permissions issue or stale socket
  ```
  [ERROR] Failed to bind socket /run/user/1000/waft/plugins/audio.sock: Address already in use
  ```
  **Fix**: Check if old plugin process is still running, remove stale socket file

- **Action handler error**: Plugin's `handle_action()` returned error
  ```
  [ERROR] Action handler error: Failed to set volume: device not found
  ```
  **Fix**: Check plugin-specific error message, verify hardware/service availability

### Overview Errors (`waft-overview`)

#### `ClientError`

Plugin client communication errors.

| Variant | Description | Recovery Strategy |
|---------|-------------|-------------------|
| `ConnectionFailed(std::io::Error)` | Failed to connect to plugin socket | Plugin not running or socket permissions wrong |
| `Timeout` | Operation exceeded 5 second timeout | Plugin unresponsive, check if hung or overloaded |
| `Transport(TransportError)` | Framing or serialization error | Protocol mismatch or corrupted message |
| `Disconnected` | Plugin closed connection unexpectedly | Plugin crashed, check plugin logs |
| `InvalidResponse(String)` | Plugin sent wrong message type | Protocol violation, check plugin implementation |
| `SocketNotFound` | Socket file doesn't exist | Plugin not started or socket removed |

**Common Scenarios:**

- **Socket not found**: Plugin daemon not running
  ```
  [WARN] Socket not found: /run/user/1000/waft/plugins/battery.sock
  ```
  **Fix**: Start the plugin daemon, check systemd service status

- **Connection timeout**: Plugin too slow to respond
  ```
  [WARN] Connection timeout for plugin 'weather' after 5s
  ```
  **Fix**: Check plugin CPU usage, network connectivity (for networked plugins)

- **Invalid response**: Protocol error
  ```
  [ERROR] Invalid response from plugin 'custom': expected SetWidgets, got UpdateWidget
  ```
  **Fix**: Update plugin to match protocol, check protocol version compatibility

#### `TransportError`

Low-level IPC transport errors (used by both sides).

| Variant | Description | Recovery Strategy |
|---------|-------------|-------------------|
| `Io(std::io::Error)` | Read/write failure on socket | Connection lost, attempt reconnect |
| `Serialization(serde_json::Error)` | Invalid JSON | Check message format, protocol version |
| `FrameTooLarge(usize)` | Message > 10MB | Reduce widget set size |

## Logging Standards

### Log Levels

| Level | Usage | Examples |
|-------|-------|----------|
| `error` | Unrecoverable errors, plugin crashes | Socket bind failed, plugin panic |
| `warn` | Recoverable errors, degraded functionality | Failed plugin init, reconnection needed |
| `info` | Important state changes | Plugin started, client connected/disconnected, plugins discovered |
| `debug` | Detailed operations | Message send/receive, socket operations |
| `trace` | *Not used* | Would be too verbose |

### Log Format

All logs follow the pattern:
```
[component] message: context
```

Examples:
```
[plugin-discovery] Found 5 plugin(s): ["audio", "battery", "bluetooth", "clock", "wifi"]
[plugin-client] Reconnected to audio (attempt 2/3)
[registry] Failed to initialize plugin 'weather': network unreachable
[plugin-server] Plugin server started: audio
```

### Structured Context

Include relevant context in logs:

- **Plugin name**: Always include when available
- **Socket path**: For connection/discovery issues
- **Attempt number**: For retry operations
- **Error kind**: Categorization for monitoring

## Recovery Strategies

### Client-Side (Overview)

#### 1. Connection Failure → Exponential Backoff

When a plugin connection fails, the client automatically retries with exponential backoff:

- Attempt 1: Immediate
- Attempt 2: After 100ms
- Attempt 3: After 200ms
- Attempt 4+: Fail permanently

```rust
// Automatic retry in PluginClient::reconnect()
let backoff = Duration::from_millis(100 * (1 << (attempt - 1)));
tokio::time::sleep(backoff).await;
```

#### 2. Timeout → Log and Continue

Timeouts (default 5 seconds) are logged but don't stop other operations:

```rust
match client.request_widgets().await {
    Err(ClientError::Timeout) => {
        log::warn!("[plugin-client] Timeout for plugin '{}'", plugin_name);
        // Continue with other plugins
    }
    // ...
}
```

#### 3. Plugin Crash → Show Error Widget

When a plugin fails initialization or crashes, overview shows an error placeholder:

```rust
// In plugin_registry.rs
let failed_widget = FailedWidget::new(name, error_msg);
registrar.register_widget(Rc::new(Widget {
    id: format!("{}:failed", name),
    slot: Slot::Info,
    weight: 999, // Show at bottom
    el: failed_widget.widget().clone().upcast::<gtk::Widget>(),
}));
```

#### 4. Invalid Response → Drop Message

Invalid plugin responses are logged but don't crash overview:

```rust
match response {
    PluginMessage::SetWidgets { widgets } => Ok(widgets),
    other => {
        log::error!("[plugin-client] Invalid response: expected SetWidgets, got {:?}", other);
        Err(ClientError::InvalidResponse(format!("{:?}", other)))
    }
}
```

### Server-Side (Plugin Daemon)

#### 1. Action Handler Error → Log and Continue

Plugin action handler errors are logged but don't stop the server:

```rust
if let Err(e) = daemon.handle_action(widget_id, action).await {
    return Err(ServerError::Other(format!("Action handler error: {}", e)));
}
// Server continues serving after returning error to client
```

#### 2. Client Disconnect → Clean Up Gracefully

Clean client disconnects are detected and handled:

```rust
if e.to_string().contains("UnexpectedEof") || e.to_string().contains("connection") {
    log::debug!("Client disconnected");
    break; // Exit client handler, server continues
}
```

#### 3. Socket Bind Failure → Helpful Error Message

Socket binding failures provide actionable guidance:

```rust
let socket_path = Self::socket_path(&plugin_name)?;
log::info!("Socket path: {}", socket_path.display());

// Ensure parent directory exists
if let Some(parent) = socket_path.parent() {
    std::fs::create_dir_all(parent)?;
    log::debug!("Created socket directory: {}", parent.display());
}

// Remove stale socket if it exists
if socket_path.exists() {
    std::fs::remove_file(&socket_path)?;
    log::debug!("Removed stale socket: {}", socket_path.display());
}
```

## Testing Error Scenarios

The IPC infrastructure includes comprehensive error scenario tests:

### Transport Layer Tests (`waft-ipc`)

- ✅ Oversized frame rejection (> 10MB)
- ✅ Invalid JSON handling
- ✅ Incomplete frame detection
- ✅ Multiple message round-trip
- ✅ Empty message handling

### Client Tests (`waft-overview`)

- ✅ Nonexistent socket detection
- ✅ Connection timeout handling
- ✅ Invalid response rejection
- ✅ Disconnect detection (UnexpectedEof)

### Discovery Tests (`waft-overview`)

- ✅ Empty directory handling
- ✅ Nonexistent directory handling
- ✅ Non-.sock file filtering
- ✅ Subdirectory handling
- ✅ Socket validation (is_socket check)

### Integration Tests

Recommended error scenarios to test:

1. **Plugin crash during action**: Verify overview continues without crash
2. **Malformed message from plugin**: Verify client rejects and logs
3. **Socket permission denied**: Verify helpful error message
4. **Concurrent client connections**: Verify server handles multiple clients
5. **Slow plugin response**: Verify timeout triggers and recovery occurs

## Troubleshooting Guide

### Plugin Won't Start

**Symptoms**: Socket file not created, bind error in logs

**Checks**:
1. Runtime directory exists: `ls /run/user/$(id -u)/waft/plugins/`
2. No stale socket: `rm /run/user/$(id -u)/waft/plugins/<plugin>.sock`
3. Permissions correct: `ls -la /run/user/$(id -u)/waft/plugins/`
4. Old process not running: `ps aux | grep <plugin>`

### Plugin Disconnects Frequently

**Symptoms**: Repeated "Client disconnected" in plugin logs

**Checks**:
1. Plugin memory usage: `ps -o rss,command -p <pid>`
2. CPU usage: `top -p <pid>`
3. Check plugin logs for errors
4. Verify plugin isn't crashing/restarting

### Overview Can't Connect to Plugin

**Symptoms**: "Socket not found" or "Connection failed" in overview logs

**Checks**:
1. Plugin running: `systemctl --user status waft-plugin-<name>`
2. Socket exists: `ls /run/user/$(id -u)/waft/plugins/<name>.sock`
3. Socket is valid: `file /run/user/$(id -u)/waft/plugins/<name>.sock`
4. Permissions: `ls -la /run/user/$(id -u)/waft/plugins/<name>.sock`

### Slow Plugin Response

**Symptoms**: Timeout errors, UI unresponsive

**Checks**:
1. Check plugin CPU usage
2. Check for blocking operations in `get_widgets()` or `handle_action()`
3. Profile plugin with `perf` or `flamegraph`
4. Consider increasing timeout: `client.set_timeout(Duration::from_secs(10))`

### Protocol Version Mismatch

**Symptoms**: Serialization errors, invalid response errors

**Checks**:
1. Verify `waft-ipc` version matches between plugin and overview
2. Check `PROTOCOL_VERSION` constant (currently `1`)
3. Rebuild plugins against current `waft-ipc` version

## Best Practices for Plugin Developers

1. **Quick Initialization**: `init()` should complete in < 1 second
2. **Fast Widget Generation**: `get_widgets()` should return in < 100ms
3. **Async Action Handlers**: Use `async` for I/O-bound operations
4. **Error Messages**: Provide actionable error messages
5. **Logging**: Use structured logs with `[plugin-name]` prefix
6. **Graceful Shutdown**: Implement proper `cleanup()` handler

## Monitoring Recommendations

For production deployments, consider monitoring:

- **Connection failure rate**: Alert if > 10% of connections fail
- **Average response time**: Alert if > 500ms
- **Plugin crash count**: Alert on any crash
- **Socket file count**: Alert if expected plugins missing
- **Memory usage**: Alert if plugin RSS > 100MB

## Future Improvements

Potential enhancements to error handling:

1. **Circuit Breaker**: Temporarily disable failing plugins
2. **Health Checks**: Periodic ping/pong to verify plugin health
3. **Metrics Export**: Prometheus/OpenMetrics endpoint
4. **Crash Recovery**: Auto-restart plugins via systemd
5. **Error Aggregation**: Collect errors from multiple plugins
6. **Rate Limiting**: Protect against misbehaving plugins

## References

- **Plugin SDK**: `crates/plugin-sdk/src/`
- **IPC Protocol**: `crates/ipc/src/message.rs`
- **Client Implementation**: `crates/overview/src/plugin_manager/client.rs`
- **Discovery**: `crates/overview/src/plugin_manager/discovery.rs`
- **Transport**: `crates/ipc/src/transport.rs`
