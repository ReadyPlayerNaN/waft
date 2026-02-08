# End-to-End IPC Testing Plan

## Overview

This document outlines the test plan for validating the complete IPC infrastructure between plugin daemons and the waft-overview application.

## Test Environment Setup

### Prerequisites
- Rust toolchain installed
- Workspace built: `cargo build --workspace`
- XDG_RUNTIME_DIR set (typically `/run/user/{uid}`)

### Component Locations
- **Plugin SDK**: `crates/plugin-sdk/`
- **Example Plugin**: `crates/plugin-sdk/examples/simple_plugin.rs`
- **Overview**: `crates/overview/`
- **IPC Protocol**: `crates/ipc/`

## Test Scenarios

### 1. Plugin Discovery and Connection

**Objective**: Verify overview can discover and connect to plugin daemons.

**Steps**:
1. Start the example plugin daemon:
   ```bash
   RUST_LOG=info cargo run --example simple_plugin
   ```
2. Verify socket creation:
   ```bash
   ls -la $XDG_RUNTIME_DIR/waft/plugins/simple.sock
   ```
3. Start overview application
4. Verify overview discovers the plugin (check logs)
5. Verify overview connects to plugin socket

**Expected Results**:
- Socket file created at correct path
- Overview logs show "discovered plugin: simple"
- Overview logs show "connected to plugin: simple"
- No connection errors in either process

**Verification**:
```bash
# Check socket exists
test -S $XDG_RUNTIME_DIR/waft/plugins/simple.sock && echo "Socket exists"

# Check processes
ps aux | grep simple_plugin
ps aux | grep waft-overview
```

---

### 2. Widget Loading and Display

**Objective**: Verify widgets from plugin daemon are loaded and displayed in overview.

**Steps**:
1. With both processes running, observe overview UI
2. Check for "Simple Plugin" feature toggle in FeatureToggles section
3. Verify widget details show "Feature is disabled"
4. Verify widget icon is displayed
5. Check overview logs for "received SetWidgets from plugin: simple"

**Expected Results**:
- Feature toggle widget appears in UI
- Widget shows correct initial state (disabled)
- Widget positioned according to weight (100)
- No rendering errors in logs

**Verification Points**:
- Widget Registry contains plugin's widgets
- UI renderer received widget data
- GTK widgets created successfully

---

### 3. User Action Routing

**Objective**: Verify user interactions are routed to the correct plugin daemon.

**Steps**:
1. Click the "Simple Plugin" toggle in the UI
2. Observe plugin daemon logs for "Received action: widget=simple:toggle, action=toggle"
3. Observe plugin daemon logs for "Toggled: enabled=true"
4. Verify UI updates to show "Feature is enabled"
5. Toggle again and verify state changes to disabled

**Expected Results**:
- Click triggers TriggerAction message to plugin
- Plugin receives action and updates state
- Plugin sends updated widgets back to overview
- UI reflects new state immediately
- No action routing errors

**Verification**:
- ActionRouter maps widget ID to plugin correctly
- PluginClient sends TriggerAction successfully
- Plugin handles action without errors
- Widget diff algorithm minimizes UI updates

---

### 4. Dynamic Widget Updates

**Objective**: Verify widget updates are properly handled.

**Steps**:
1. Toggle the feature on
2. Verify details text changes from "disabled" to "enabled"
3. Verify active state visual changes (GTK toggle state)
4. Check diff algorithm logs for efficient updates
5. Verify only changed widget properties are updated

**Expected Results**:
- Only modified properties trigger GTK updates
- No full UI re-render on state change
- Smooth visual transitions
- Diff algorithm logs show minimal changes

**Verification**:
- WidgetDiff correctly identifies changed fields
- UI renderer applies only necessary updates
- No memory leaks from widget updates

---

### 5. Plugin Disconnection Handling

**Objective**: Verify graceful handling of plugin daemon shutdown.

**Steps**:
1. With plugin running, stop it (Ctrl+C)
2. Observe overview logs for "plugin disconnected: simple"
3. Verify widget is removed from UI or marked as unavailable
4. Attempt to toggle the widget (if still shown)
5. Verify overview shows appropriate error state

**Expected Results**:
- Overview detects disconnection
- Widget registry removes plugin's widgets
- UI updates to remove or disable widgets
- No crashes or panics
- ActionRouter returns PluginNotConnected error

**Verification**:
- Socket file removed or stale
- PluginClient detects disconnect
- WidgetRegistry cleaned up
- UI remains stable

---

### 6. Plugin Reconnection

**Objective**: Verify plugin can reconnect after disconnection.

**Steps**:
1. Stop plugin daemon
2. Wait for overview to detect disconnection
3. Restart plugin daemon: `cargo run --example simple_plugin`
4. Verify overview rediscovers plugin
5. Verify widgets reappear in UI
6. Test widget interaction works

**Expected Results**:
- Plugin discovery detects new socket
- Overview reconnects to plugin
- Widgets restored in UI
- Action routing works after reconnection
- Previous state may be lost (expected)

**Verification**:
- Discovery loop finds new socket
- PluginClient successfully reconnects
- WidgetRegistry re-populated
- No stale state from previous connection

---

### 7. Multiple Widget Types

**Objective**: Verify different widget types render and function correctly.

**Test Widgets** (can extend simple_plugin):
- FeatureToggle (already in simple_plugin)
- Slider (for volume/brightness)
- Button (for actions)
- Label (for status)
- Container (for expandable content)

**Steps**: For each widget type:
1. Add widget to plugin's `get_widgets()`
2. Verify rendering in appropriate slot
3. Test interactions (click, drag, etc.)
4. Verify action handling

**Expected Results**:
- All widget types render correctly
- Interactions work as expected
- Actions route properly
- UI updates reflect state changes

---

### 8. Socket Permission and Security

**Objective**: Verify socket security and permissions.

**Steps**:
1. Check socket file permissions:
   ```bash
   ls -l $XDG_RUNTIME_DIR/waft/plugins/simple.sock
   ```
2. Verify socket is user-owned
3. Verify socket is in user's runtime directory
4. Test connection from different user (should fail)

**Expected Results**:
- Socket permissions: `srwx------` (user only)
- Socket owned by running user
- Other users cannot connect
- Socket in secure runtime directory

---

### 9. Message Protocol Validation

**Objective**: Verify IPC messages are correctly formatted and processed.

**Steps**:
1. Enable debug logging: `RUST_LOG=debug`
2. Observe message logs for both processes
3. Verify length-prefixed framing
4. Verify JSON serialization
5. Check for protocol version compatibility

**Expected Results**:
- Messages use 4-byte big-endian length prefix
- JSON payload is valid
- No serialization errors
- Protocol version matches (v1)
- Messages under 10MB size limit

**Verification**:
- transport::read_framed succeeds
- No TransportError::FrameTooLarge
- No TransportError::Serialization
- All message types handled

---

### 10. Error Handling and Recovery

**Objective**: Verify robust error handling throughout the pipeline.

**Test Cases**:
1. **Invalid action ID**: Send unknown action, verify graceful handling
2. **Malformed message**: (requires manual testing) verify rejection
3. **Socket timeout**: Simulate slow plugin, verify timeout handling
4. **Rapid state changes**: Toggle rapidly, verify no race conditions
5. **Memory pressure**: Monitor memory usage over time

**Expected Results**:
- Invalid actions logged but don't crash
- Malformed messages rejected safely
- Timeouts trigger appropriate errors
- No race conditions or deadlocks
- No memory leaks

---

## Automated Test Structure

### Integration Test File
Location: `crates/overview/tests/ipc_integration_test.rs`

```rust
#[tokio::test]
async fn test_plugin_connection_lifecycle() {
    // 1. Start mock plugin server
    // 2. Start overview plugin manager
    // 3. Verify discovery
    // 4. Verify connection
    // 5. Stop plugin
    // 6. Verify disconnection handling
}

#[tokio::test]
async fn test_widget_action_roundtrip() {
    // 1. Setup plugin + overview
    // 2. Load widgets
    // 3. Trigger action
    // 4. Verify state update
}

#[tokio::test]
async fn test_multiple_plugins() {
    // 1. Start multiple plugins
    // 2. Verify all discovered
    // 3. Test action routing to correct plugin
    // 4. Verify no cross-contamination
}
```

### Unit Test Coverage
- [x] PluginServer message handling
- [x] PluginClient connection/reconnection
- [x] ActionRouter widget mapping
- [x] WidgetRegistry operations
- [x] Widget diff algorithm
- [x] Transport framing

---

## Performance Benchmarks

### Metrics to Track
1. **Plugin Discovery Time**: < 100ms per plugin
2. **Connection Establishment**: < 50ms
3. **Widget Load Time**: < 200ms for full widget set
4. **Action Roundtrip**: < 100ms (send action → receive update)
5. **UI Update Latency**: < 50ms from widget update to screen

### Profiling Commands
```bash
# Profile plugin daemon
RUST_LOG=trace cargo run --example simple_plugin

# Profile overview with IPC enabled
RUST_LOG=trace cargo run -p waft-overview

# Memory usage
ps aux | grep simple_plugin
ps aux | grep waft-overview
```

---

## Troubleshooting Guide

### Socket Not Created
- Check XDG_RUNTIME_DIR is set
- Verify write permissions to runtime directory
- Check for stale socket files
- Verify plugin daemon started successfully

### Connection Refused
- Verify socket file exists
- Check socket permissions
- Ensure plugin daemon is running
- Verify no firewall/SELinux blocking

### Widgets Not Appearing
- Check WidgetRegistry logs
- Verify SetWidgets message received
- Check widget slot matches UI section
- Verify UI renderer is processing updates

### Actions Not Working
- Check ActionRouter widget mapping
- Verify plugin client registered
- Check TriggerAction message sent
- Verify plugin handle_action() implementation

### Performance Issues
- Check for excessive widget updates
- Verify diff algorithm efficiency
- Monitor socket I/O latency
- Check for blocking operations in main thread

---

## Success Criteria

✅ All test scenarios pass
✅ No crashes or panics
✅ No memory leaks over 1 hour runtime
✅ Performance metrics within targets
✅ Error messages are clear and actionable
✅ Plugin can be stopped and restarted without issues
✅ Multiple plugins can coexist
✅ UI remains responsive under load

---

## Next Steps After E2E Testing

1. **Performance Optimization**: Profile hot paths, optimize diff algorithm
2. **Real Plugin Migration**: Convert existing plugins to daemon model
3. **Production Hardening**: Add retry logic, better error recovery
4. **Monitoring**: Add metrics collection for production debugging
5. **Documentation**: User guide for plugin developers

---

## Test Execution Checklist

- [ ] Environment setup complete
- [ ] Plugin daemon builds successfully
- [ ] Overview builds successfully
- [ ] Scenario 1: Discovery and connection ✓
- [ ] Scenario 2: Widget loading ✓
- [ ] Scenario 3: Action routing ✓
- [ ] Scenario 4: Dynamic updates ✓
- [ ] Scenario 5: Disconnection handling ✓
- [ ] Scenario 6: Reconnection ✓
- [ ] Scenario 7: Multiple widget types ✓
- [ ] Scenario 8: Security validation ✓
- [ ] Scenario 9: Protocol validation ✓
- [ ] Scenario 10: Error handling ✓
- [ ] Performance benchmarks collected
- [ ] Integration tests written and passing
- [ ] Documentation updated

---

**Document Version**: 1.0
**Last Updated**: 2026-02-08
**Author**: sdk-server-dev
**Status**: Ready for Task #12 execution
