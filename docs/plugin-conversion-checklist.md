# Plugin Conversion Checklist (Remaining 12 Plugins)

This checklist tracks the conversion of all plugins from cdylib shared libraries to IPC daemons.

**Last Updated**: 2026-02-09

## Conversion Status

| Plugin | Tier | Status | Complexity | Estimated Hours | Notes |
|--------|------|--------|------------|----------------|-------|
| clock | 1 | ✅ **DONE** | Low | 4 (actual) | Reference implementation |
| darkman | 1 | ⏳ Pending | Low | 4 | Simple toggle, D-Bus listener |
| caffeine | 1 | ⏳ Pending | Low | 4 | Simple toggle, D-Bus calls |
| battery | 2 | ⏳ Pending | Medium | 8 | UPower D-Bus, state tracking |
| brightness | 2 | ⏳ Pending | Medium | 8 | D-Bus + backlight sysfs |
| systemd-actions | 2 | ⏳ Pending | Medium | 8 | Systemd D-Bus API |
| audio | 3 | ⏳ Pending | High | 16 | PulseAudio/PipeWire state |
| networkmanager | 3 | ⏳ Pending | High | 16 | nmrs library integration |
| blueman | 3 | ⏳ Pending | High | 16 | BlueZ D-Bus, device pairing |
| notifications | 4 | ⏳ Pending | Very High | 24+ | Custom UI, runtime bridge |
| eds | 4 | ⏳ Pending | Very High | 24+ | Calendar data, recurring events |
| weather | 4 | ⏳ Pending | Very High | 24+ | Network requests, caching |
| keyboard-layout | 1 | ⏳ Pending | Low | 4 | XKB state, D-Bus signals |

**Progress**: 1/13 complete (7.7%)
**Total Estimated**: ~152 hours (with 1.5x buffer = 228 hours)

## Tier Definitions

### Tier 1: Simple (Low Complexity)
- Single toggle or static display
- Minimal external dependencies
- No complex state management
- **Examples**: clock, darkman, caffeine, keyboard-layout

### Tier 2: Medium (External Services)
- D-Bus communication
- System service integration
- Moderate state tracking
- **Examples**: battery, brightness, systemd-actions

### Tier 3: High (State Management)
- Complex state synchronization
- Heavy external libraries
- Multiple concurrent operations
- **Examples**: audio, networkmanager, blueman

### Tier 4: Very High (Custom UI/Complex Logic)
- Custom rendering requirements
- Asynchronous data processing
- Cache management
- **Examples**: notifications, eds, weather

---

## Detailed Plugin Breakdown

### ✅ clock (COMPLETED)
**Status**: Done (Phase 5.1)
**Time Spent**: 4 hours
**Reference**: `/home/just-paja/Work/shell/sacrebleui/plugins/clock/`

**Widgets**:
- Header: Date + time display (updates every second)

**Actions**:
- Click: Launch calendar app (configurable)

**Config**:
```toml
[[plugins]]
id = "waft::clock-daemon"
on_click = "gnome-calendar"
```

**Lessons**:
- ✅ Builder pattern works excellently
- ✅ Widget protocol sufficient for all needs
- ✅ Config loading straightforward
- ⚠️ Time updates every second - consider optimization

---

### ⏳ darkman (Tier 1)

**Current Type**: cdylib
**Current File**: `/home/just-paja/Work/shell/sacrebleui/plugins/darkman/src/lib.rs`
**Daemon File**: `plugins/darkman/bin/waft-darkman-daemon.rs` (to be created)

**Widgets**:
- FeatureToggle: Dark mode on/off

**Actions**:
- Toggle: Switch between light/dark modes

**External Dependencies**:
- D-Bus: `org.freedesktop.portal.Settings` (dark mode preference)

**Migration Notes**:
- Listen for portal settings changes
- Use `tokio::task::spawn()` for D-Bus signals (no TLS issues!)
- State: single boolean (is_dark)

**Conversion Steps**:
- [ ] Create daemon binary
- [ ] Implement PluginDaemon trait
- [ ] Convert toggle widget to FeatureToggleBuilder
- [ ] Add D-Bus signal listener
- [ ] Test mode switching
- [ ] Create systemd service
- [ ] Test auto-start

**Estimated Time**: 4 hours

---

### ⏳ caffeine (Tier 1)

**Current Type**: cdylib
**Current File**: `/home/just-paja/Work/shell/sacrebleui/plugins/caffeine/src/lib.rs`
**Daemon File**: `plugins/caffeine/bin/waft-caffeine-daemon.rs` (to be created)

**Widgets**:
- FeatureToggle: Caffeine mode (prevent sleep)

**Actions**:
- Toggle: Enable/disable sleep inhibitor

**External Dependencies**:
- D-Bus: `org.freedesktop.ScreenSaver.Inhibit/Uninhibit`

**Migration Notes**:
- Track inhibitor cookie
- Handle D-Bus disconnection (cookie becomes invalid)
- State: Option<u32> (inhibit cookie)

**Conversion Steps**:
- [ ] Create daemon binary
- [ ] Implement PluginDaemon trait
- [ ] Convert toggle widget
- [ ] Add D-Bus inhibit/uninhibit calls
- [ ] Handle cookie lifecycle
- [ ] Test inhibitor activation
- [ ] Create systemd service

**Estimated Time**: 4 hours

---

### ⏳ keyboard-layout (Tier 1)

**Current Type**: cdylib
**Current File**: `/home/just-paja/Work/shell/sacrebleui/plugins/keyboard-layout/src/lib.rs`
**Daemon File**: `plugins/keyboard-layout/bin/waft-keyboard-layout-daemon.rs` (to be created)

**Widgets**:
- Label: Current keyboard layout (e.g., "us", "cz")

**Actions**:
- Click: Open keyboard settings (optional)

**External Dependencies**:
- D-Bus: `org.gnome.Shell` (keyboard layout signals)
- XKB: Layout state

**Migration Notes**:
- Listen for layout change signals
- Update widget on layout switch
- State: String (current layout)

**Conversion Steps**:
- [ ] Create daemon binary
- [ ] Implement PluginDaemon trait
- [ ] Convert label widget
- [ ] Add D-Bus signal listener for layout changes
- [ ] Test layout switching
- [ ] Create systemd service

**Estimated Time**: 4 hours

---

### ⏳ battery (Tier 2)

**Current Type**: cdylib
**Current File**: `/home/just-paja/Work/shell/sacrebleui/plugins/battery/src/lib.rs`
**Daemon File**: `plugins/battery/bin/waft-battery-daemon.rs` (to be created)

**Widgets**:
- Label/Icon: Battery percentage + charging status

**Actions**:
- Click: Open power settings (optional)

**External Dependencies**:
- D-Bus: UPower (`org.freedesktop.UPower.Device`)
- Properties: Percentage, State, TimeToEmpty, TimeToFull

**Migration Notes**:
- Poll UPower for battery state
- Listen for property changes
- Handle multiple batteries
- State: Vec<BatteryInfo>

**Conversion Steps**:
- [ ] Create daemon binary
- [ ] Implement PluginDaemon trait
- [ ] Convert battery widget(s)
- [ ] Add UPower D-Bus client
- [ ] Listen for property changes
- [ ] Test charging/discharging states
- [ ] Test multiple batteries (if available)
- [ ] Create systemd service

**Estimated Time**: 8 hours

---

### ⏳ brightness (Tier 2)

**Current Type**: cdylib
**Current File**: `/home/just-paja/Work/shell/sacrebleui/plugins/brightness/src/lib.rs`
**Daemon File**: `plugins/brightness/bin/waft-brightness-daemon.rs` (to be created)

**Widgets**:
- Slider: Screen brightness (0-100%)

**Actions**:
- ValueChange: Set brightness level
- IconClick: Toggle automatic brightness (optional)

**External Dependencies**:
- sysfs: `/sys/class/backlight/*/brightness`
- D-Bus: `org.freedesktop.login1.Session` (brightness control)

**Migration Notes**:
- Read/write brightness via sysfs or D-Bus
- Handle permissions (may need polkit)
- Watch for external brightness changes
- State: f64 (current brightness 0.0-1.0)

**Conversion Steps**:
- [ ] Create daemon binary
- [ ] Implement PluginDaemon trait
- [ ] Convert slider widget
- [ ] Add brightness read/write logic
- [ ] Test brightness adjustment
- [ ] Handle permission errors gracefully
- [ ] Create systemd service

**Estimated Time**: 8 hours

---

### ⏳ systemd-actions (Tier 2)

**Current Type**: cdylib
**Current File**: `/home/just-paja/Work/shell/sacrebleui/plugins/systemd-actions/src/lib.rs`
**Daemon File**: `plugins/systemd-actions/bin/waft-systemd-actions-daemon.rs` (to be created)

**Widgets**:
- Buttons: Shutdown, Reboot, Logout, Suspend

**Actions**:
- Click: Trigger systemd action

**External Dependencies**:
- D-Bus: `org.freedesktop.login1.Manager` (systemd-logind)
- Methods: PowerOff, Reboot, Suspend, Hibernate

**Migration Notes**:
- Call systemd D-Bus methods on button click
- Handle polkit authentication
- Show confirmation dialog (optional)
- State: None (stateless)

**Conversion Steps**:
- [ ] Create daemon binary
- [ ] Implement PluginDaemon trait
- [ ] Convert button widgets
- [ ] Add systemd D-Bus client
- [ ] Test each action (suspend, reboot, etc.)
- [ ] Handle polkit prompts
- [ ] Create systemd service

**Estimated Time**: 8 hours

---

### ⏳ audio (Tier 3)

**Current Type**: cdylib
**Current File**: `/home/just-paja/Work/shell/sacrebleui/plugins/audio/src/lib.rs`
**Daemon File**: `plugins/audio/bin/waft-audio-daemon.rs` (to be created)

**Widgets**:
- Slider: Volume control
- FeatureToggle: Mute toggle
- MenuRows: Input/output device selection

**Actions**:
- ValueChange: Set volume
- Toggle: Mute/unmute
- Click: Switch audio device

**External Dependencies**:
- PulseAudio or PipeWire via D-Bus/native API
- Properties: Volume, Mute, Devices

**Migration Notes**:
- Complex state synchronization (volume, mute, device list)
- Listen for audio server events
- Handle server restart
- State: AudioState { volume, muted, devices }

**Conversion Steps**:
- [ ] Create daemon binary
- [ ] Implement PluginDaemon trait
- [ ] Convert slider + toggle widgets
- [ ] Add PulseAudio/PipeWire client
- [ ] Listen for volume/device changes
- [ ] Test volume adjustment
- [ ] Test mute toggle
- [ ] Test device switching
- [ ] Handle audio server restart
- [ ] Create systemd service

**Estimated Time**: 16 hours

---

### ⏳ networkmanager (Tier 3)

**Current Type**: cdylib
**Current File**: `/home/just-paja/Work/shell/sacrebleui/plugins/networkmanager/src/lib.rs`
**Daemon File**: `plugins/networkmanager/bin/waft-networkmanager-daemon.rs` (to be created)

**Widgets**:
- FeatureToggle: Wi-Fi on/off
- MenuRows: Network list (SSID, signal strength)
- FeatureToggle: Airplane mode (optional)

**Actions**:
- Toggle: Enable/disable Wi-Fi
- Click: Connect to network

**External Dependencies**:
- NetworkManager via D-Bus (`org.freedesktop.NetworkManager`)
- nmrs library (Rust bindings)

**Migration Notes**:
- Heavy state tracking (networks, connections, devices)
- Use existing nmrs library
- Handle network scanning
- Runtime bridge for nmrs (cdylib tokio pattern no longer needed!)
- State: NetworkState { devices, connections, access_points }

**Conversion Steps**:
- [ ] Create daemon binary
- [ ] Implement PluginDaemon trait
- [ ] Convert Wi-Fi toggle + network list widgets
- [ ] Integrate nmrs library (NO runtime bridge needed!)
- [ ] Listen for network changes
- [ ] Test Wi-Fi toggle
- [ ] Test network scanning
- [ ] Test connection to network
- [ ] Handle connection failures
- [ ] Create systemd service

**Estimated Time**: 16 hours

---

### ⏳ blueman (Tier 3)

**Current Type**: cdylib
**Current File**: `/home/just-paja/Work/shell/sacrebleui/plugins/blueman/src/lib.rs`
**Daemon File**: `plugins/blueman/bin/waft-blueman-daemon.rs` (to be created)

**Widgets**:
- FeatureToggle: Bluetooth on/off
- MenuRows: Device list (name, status, battery)
- Buttons: Pair, Forget device

**Actions**:
- Toggle: Enable/disable Bluetooth
- Click: Connect/disconnect device
- Pair: Trigger pairing mode

**External Dependencies**:
- BlueZ via D-Bus (`org.bluez`)
- Properties: Devices, Adapters, Powered

**Migration Notes**:
- Device pairing flow (multi-step)
- Track device connection state
- Handle pairing PIN/passkey
- State: BluetoothState { powered, devices, adapter }

**Conversion Steps**:
- [ ] Create daemon binary
- [ ] Implement PluginDaemon trait
- [ ] Convert Bluetooth toggle + device list widgets
- [ ] Add BlueZ D-Bus client
- [ ] Listen for device changes
- [ ] Test Bluetooth toggle
- [ ] Test device discovery
- [ ] Test device pairing
- [ ] Test device connection
- [ ] Handle pairing failures
- [ ] Create systemd service

**Estimated Time**: 16 hours

---

### ⏳ notifications (Tier 4)

**Current Type**: cdylib + D-Bus server
**Current File**: `/home/just-paja/Work/shell/sacrebleui/plugins/notifications/src/lib.rs`
**Daemon File**: `plugins/notifications/bin/waft-notifications-daemon.rs` (to be created)

**Widgets**:
- Custom notification popups (NOT in standard widget protocol)
- MenuRows: Notification list in overview

**Actions**:
- Click: Open notification
- Dismiss: Close notification

**External Dependencies**:
- D-Bus server: `org.freedesktop.Notifications` (receives notifications)
- Runtime bridge (tokio runtime on separate thread)

**Migration Notes**:
- **CRITICAL**: Already has plugin-local tokio runtime (runtime bridge pattern)
- Custom popup rendering (may need protocol extension)
- Notification storage and persistence
- 79 unit tests to maintain
- State: NotificationStore { active, dismissed, history }

**Conversion Steps**:
- [ ] Create daemon binary
- [ ] Implement PluginDaemon trait
- [ ] Design custom widget protocol for notifications
- [ ] Convert notification list to MenuRows
- [ ] Migrate runtime bridge to daemon
- [ ] Maintain D-Bus server
- [ ] Test notification reception
- [ ] Test notification display
- [ ] Test notification actions
- [ ] Run all 79 tests
- [ ] Create systemd service

**Estimated Time**: 24+ hours

**Special Considerations**:
- May need popup window outside IPC protocol
- Consider separate notification daemon + plugin client
- Protocol extension for custom notification widgets

---

### ⏳ eds (Tier 4)

**Current Type**: cdylib
**Current File**: `/home/just-paja/Work/shell/sacrebleui/plugins/eds/src/lib.rs`
**Daemon File**: `plugins/eds/bin/waft-eds-daemon.rs` (to be created)

**Widgets**:
- Calendar widget (custom rendering)
- Event list (MenuRows)
- Event details (Container)

**Actions**:
- Click: Open event details
- DateSelect: Show events for date

**External Dependencies**:
- Evolution Data Server (EDS) via D-Bus
- libical for iCalendar parsing
- Recurring event calculation

**Migration Notes**:
- Complex calendar rendering (may need custom widget)
- Recurring event logic
- Cache management (events by date range)
- State: AgendaState { events, selected_date, calendars }

**Conversion Steps**:
- [ ] Create daemon binary
- [ ] Implement PluginDaemon trait
- [ ] Design calendar widget protocol extension
- [ ] Convert event list to MenuRows
- [ ] Add EDS D-Bus client
- [ ] Implement recurring event logic
- [ ] Test event fetching
- [ ] Test calendar navigation
- [ ] Test recurring events
- [ ] Handle EDS connection issues
- [ ] Create systemd service

**Estimated Time**: 24+ hours

**Special Considerations**:
- Calendar widget likely needs protocol extension
- Recurring events are complex (RRULE parsing)
- Consider read-only mode initially

---

### ⏳ weather (Tier 4)

**Current Type**: cdylib
**Current File**: `/home/just-paja/Work/shell/sacrebleui/plugins/weather/src/lib.rs`
**Daemon File**: `plugins/weather/bin/waft-weather-daemon.rs` (to be created)

**Widgets**:
- Weather icon + temperature
- Forecast list (MenuRows)
- Details (humidity, wind, etc.)

**Actions**:
- Click: Open detailed forecast
- Refresh: Force weather update

**External Dependencies**:
- HTTP API: OpenWeatherMap / Weather.gov
- Geolocation (IP or GPS)
- Network access

**Migration Notes**:
- HTTP requests (use reqwest)
- Data caching (avoid rate limits)
- Handle network errors gracefully
- Background update loop
- State: WeatherState { current, forecast, last_update }

**Conversion Steps**:
- [ ] Create daemon binary
- [ ] Implement PluginDaemon trait
- [ ] Convert weather widgets
- [ ] Add HTTP client (reqwest)
- [ ] Implement caching logic
- [ ] Test weather fetching
- [ ] Test offline mode
- [ ] Test API rate limiting
- [ ] Handle network errors
- [ ] Create systemd service

**Estimated Time**: 24+ hours

**Special Considerations**:
- Need API key configuration
- Privacy considerations (location)
- Offline mode UX

---

## Conversion Workflow

For each plugin, follow this process:

### 1. Preparation
- [ ] Read plugin-daemon-migration-guide.md
- [ ] Study current plugin implementation
- [ ] List all widgets and actions
- [ ] Identify external dependencies
- [ ] Create feature branch: `phase5-<plugin>-daemon`

### 2. Implementation
- [ ] Create daemon binary file
- [ ] Define daemon state struct
- [ ] Implement config loading
- [ ] Implement PluginDaemon trait
- [ ] Convert widgets to protocol
- [ ] Handle actions
- [ ] Add logging

### 3. Testing
- [ ] Build daemon: `cargo build --bin waft-<plugin>-daemon`
- [ ] Run manually: `RUST_LOG=debug ./target/debug/waft-<plugin>-daemon`
- [ ] Verify socket creation
- [ ] Connect overview and test all features
- [ ] Add unit tests
- [ ] Add integration tests

### 4. Integration
- [ ] Create systemd service file
- [ ] Test systemd service installation
- [ ] Update Cargo.toml
- [ ] Update documentation
- [ ] Create PR

### 5. Stabilization
- [ ] Run for 24 hours in production
- [ ] Monitor for crashes
- [ ] Check resource usage
- [ ] Verify all features work
- [ ] Remove old cdylib code (optional)

### 6. Cleanup
- [ ] Mark plugin complete in this checklist
- [ ] Document lessons learned
- [ ] Update MEMORY.md if needed

---

## Success Criteria

A plugin conversion is complete when:

- ✅ Daemon binary builds without errors
- ✅ Socket created at correct path
- ✅ Overview connects successfully
- ✅ All widgets render correctly
- ✅ All actions work as expected
- ✅ Config loading works
- ✅ Unit tests pass
- ✅ Integration tests pass
- ✅ Systemd service starts automatically
- ✅ No regressions vs cdylib version
- ✅ 24hr stability test passes

---

## Notes & Common Issues

### Issue: Socket Permission Denied
**Solution**: Check `XDG_RUNTIME_DIR` permissions, verify directory exists

### Issue: Daemon Exits Immediately
**Solution**: Check journalctl logs, verify dependencies installed

### Issue: Overview Can't Connect
**Solution**: Verify socket path, check daemon is running, test with netcat

### Issue: Widget Not Updating
**Solution**: Verify get_widgets() called, check action handling returns correctly

### Issue: Config Not Loading
**Solution**: Verify TOML syntax, check plugin ID matches, use `unwrap_or_default()`

---

## Timeline

**Optimistic** (no blockers): 10 weeks
**Realistic** (some issues): 15 weeks
**Pessimistic** (major issues): 20 weeks

**Current Pace**: 1 plugin complete in 4 hours (clock)
**Average Expected**: 12 hours/plugin (including testing, docs)

---

**Last Updated**: 2026-02-09
**Next Review**: After 3 conversions complete
