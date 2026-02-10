# 1. Cleanup slider

- Rename property `muted` to `disabled`
- Rename property `base_icon` to `icon`
- Replace custom implementation of chevron icon with MenuChevron widget
- Deduplicate code in slider

# 4. Migrate eds-agenda

Migrate plugin eds-agenda to daemon architecture.

## Component mappings

Existing GTK components map to protocol widgets:

- AttendeeRow → ListRow (status icon + name label as children)
- AttendeeList → IconList (section icon "system-users-symbolic" + attendee ListRows as children)
- AgendaDetails → IconList (each detail section: location, attendees, description)
- MeetingButton → ListButton (one per meeting link, show all links)

## New protocol widgets required

### Details

Expandable section with summary + hidden content. Expansion is managed by the
overview MenuStore, same as FeatureToggle. The renderer creates an internal
gtk::Revealer and MenuChevron — these are not exposed in the protocol.

Protocol fields:

```rust
Details {
    summary: Box<Widget>,       // Always-visible content
    content: Box<Widget>,       // Hidden until expanded
    css_classes: Vec<String>,   // For past/ongoing styling
    on_toggle: Action,          // Fires with ActionParams::Value(1.0) on expand, 0.0 on collapse
}
```

The renderer uses the widget_id to derive a stable MenuStore menu ID (same as
FeatureToggle). Internally, it composes:

```
<Row hexpand>
  {summary}
  <Button><MenuChevron /></Button>
</Row>
<Revealer>
  {content}
</Revealer>
```

The chevron and revealer are managed by the MenuStore subscription. The
on_toggle action notifies the daemon of expand/collapse state changes.

### ToggleButton

A button that can be active/inactive (for "show past events" toggle).

```rust
ToggleButton {
    icon: String,
    active: bool,
    on_toggle: Action,          // Fires with ActionParams::Value(1.0/0.0)
}
```

### Separator

Simple horizontal divider line.

```rust
Separator {}
```

## Agenda event card structure

Each event with details wraps in a Details widget:

```
<Details css_classes={past/ongoing} on_toggle={toggle_detail}>
  summary:
    <Row>
      <Label css_classes=["dim-label", "caption"] width_chars=13>{time_range}</Label>
      <Label hexpand ellipsize>{title}</Label>
      <ListButton>{provider_label}</ListButton>   <!-- one per meeting link -->
    </Row>
  content:
    <Col>
      <IconList icon="mark-location-symbolic">
        <Label>{location}</Label>
      </IconList>
      <IconList icon="system-users-symbolic">
        <IconList icon={rsvp_icon} icon_size=12>
          <Label>{attendee_name}</Label>
        </IconList>
        ...
      </IconList>
      <IconList icon="text-x-generic-symbolic">
        <Label>{description_truncated}</Label>
      </IconList>
    </Col>
</Details>
```

Events without details render as a plain Row (no Details wrapper).

## Top-level agenda widget structure

```
<Col>
  <Row>                                          <!-- header -->
    <Label css_classes=["title-3"] hexpand>{agenda_title}</Label>
    <ToggleButton icon="task-past-due-symbolic" active={show_past} on_toggle={toggle_past} />
  </Row>

  <!-- when show_past=true, daemon includes past events + separator -->
  {...past_event_details}
  <Separator />

  <!-- period separator between today and tomorrow -->
  <Label css_classes=["dim-label"]>{period_date_label}</Label>

  {...present_or_future_event_details}
</Col>
```

The daemon controls past event visibility by including/excluding them from the
widget tree. When the user toggles "show past", the daemon receives the action,
updates its state, and re-sends all widgets. No protocol-level Revealer needed.

## Implementation notes

- Switch time formatting from glib::DateTime to chrono::Local (daemon has no glib)
- Reuse values.rs (iCal parsing, recurring events, timezone) and store.rs as-is — pure Rust, no GTK
- Adapt dbus.rs: remove spawn_on_tokio bridge, use direct tokio spawns
- Replace Rc<RefCell<T>> with Arc<StdMutex<T>> for Send+Sync daemon trait
- Use PluginDaemon trait with WidgetNotifier for push updates on EDS signal changes

# 5. Migrate notifications to daemon & multi-app architecture

The notifications plugin is the last complex cdylib. Its migration is intertwined
with a larger architectural change: splitting the monolithic overview into multiple
apps orchestrated by a central waft daemon.

## Architecture: waft daemon hub

Currently plugins connect directly to waft-overview via Unix sockets. This doesn't
scale to multiple apps (waft-overview, waft-toasts, waft-settings). Instead:

```
                    ┌─────────────────┐
                    │   waft daemon    │  (central hub)
                    │                  │
                    │  Plugin registry │
                    │  Subscriber mgmt│
                    │  Action routing  │
                    └──┬───┬───┬──────┘
                       │   │   │
          ┌────────────┘   │   └────────────┐
          ▼                ▼                 ▼
    ┌───────────┐   ┌───────────┐    ┌───────────┐
    │  plugins   │   │  plugins  │    │  plugins  │
    └───────────┘   └───────────┘    └───────────┘

          ▲                ▲                 ▲
          │                │                 │
    ┌─────┴─────┐   ┌─────┴─────┐    ┌─────┴─────┐
    │waft-overview│  │waft-toasts │    │waft-settings│
    └───────────┘   └───────────┘    └───────────┘
```

- Plugins send widgets to the waft daemon
- Apps connect to waft daemon and subscribe to plugins they care about
- Plugins only produce widgets when they have subscribers (saves CPU when no UI is open)
- Actions from apps flow back through the daemon to the right plugin
- The daemon spawns plugin processes and manages their lifecycle

## Protocol improvements

### Subscriber model

Plugins should not produce widgets until an app subscribes. This avoids wasted
work when the overlay is closed or no app is running.

New messages:

```
App → Daemon:
  Subscribe { plugin_ids: Vec<String> }   // "I want widgets from these plugins"
  Unsubscribe { plugin_ids: Vec<String> } // "I no longer need these"

Daemon → Plugin:
  SubscriberCount { count: usize }        // "N apps want your widgets"

Plugin side:
  PluginDaemon gets fn on_subscribers_changed(count: usize)
  PluginServer tracks subscriber count, calls on_subscribers_changed
  When count goes 0 → plugin can pause expensive work (D-Bus monitoring, timers)
  When count goes >0 → plugin resumes and pushes current state
```

### App registration

Apps identify themselves when connecting:

```
App → Daemon:
  Register { app_id: String, capabilities: Vec<String> }
  // capabilities: ["widgets", "notifications", "actions"]
```

## Notification UI: approaches for specialized rendering

The notification card is highly specialized (Pango markup, countdown bar, action
buttons, urgency styling, grouped by app). Three approaches:

### Option A: NotificationCard as a protocol widget (recommended)

Add a first-class `Widget::NotificationCard` to the protocol. The daemon sends
structured notification data; each app's renderer creates the appropriate GTK
representation.

```rust
NotificationCard {
    notification_id: u64,
    app_name: String,
    app_icon: Option<String>,
    title: String,              // May contain Pango markup
    body: Option<String>,       // May contain Pango markup
    actions: Vec<NotificationAction>,  // { key, label }
    urgency: u8,                // 0=low, 1=normal, 2=critical
    timestamp: i64,             // Unix timestamp of arrival
    resident: bool,             // If true, doesn't auto-expire
    on_action: Action,          // ActionParams::Text(action_key)
    on_dismiss: Action,
}
```

- waft-toasts renders this as a toast (countdown bar, slide animation, auto-dismiss)
- waft-overview renders this as a panel card (grouped, persistent, expandable)
- Each app has its own GTK rendering for NotificationCard — not shared

Pro: Clean data/rendering separation, each app renders optimally for its context.
Con: NotificationCard is domain-specific in a general protocol.

### Option B: Enhance Label + add ProgressBar + compose

Extend existing protocol widgets to be capable enough:

- `Label`: add `markup: bool`, `wrap: bool`, `ellipsize: bool`
- Add `ProgressBar { fraction: f64, css_classes: Vec<String> }`
- Compose notification cards from Row/Col/Label/Button/ProgressBar

Pro: General-purpose improvements, no domain-specific widgets.
Con: Verbose widget trees, daemon duplicates rendering logic across toast/panel,
fragile composition.

### Option C: Separate notification data channel

Notifications aren't really "widgets" — they're data rendered differently per
context. The daemon sends `NotificationData` messages alongside `SetWidgets`.
Each app has a built-in notification renderer.

Pro: Maximum flexibility per app.
Con: Parallel protocol path, more complex daemon routing.

### Recommendation

Option A (NotificationCard) is the pragmatic choice. Notifications are a
well-defined domain with a freedesktop spec. Making it a first-class widget
avoids the combinatorial explosion of Option B and the protocol complexity of
Option C. The per-app rendering is expected — toasts and panel cards ARE
fundamentally different views of the same data.

## Tasks

### Phase 1: Extract shared infrastructure from overview

These extractions must happen before multi-app support, so that waft-toasts and
other apps can reuse the same infrastructure.

#### 1a. Create waft-plugin-client crate

Extract from `crates/overview/src/plugin_manager/`:

- `client.rs` → IPC socket client (split read/write, poll-based write thread)
- `router.rs` → action routing (widget_id → plugin_id mapping)
- `discovery.rs` → socket discovery (scan /run/user/{uid}/waft/plugins/)
- `registry.rs` → widget registry (plugin_id → widget_id → NamedWidget)
- `manager.rs` → plugin manager (event-driven IPC coordinator)

This code is already generic. Needs: parameterized socket path, pub mod exports.

#### 1b. Make daemon spawner configurable

Currently `daemon_spawner.rs` has a hardcoded list of 11 daemon binaries. Extract
to a shared module that accepts a config-driven plugin list. The waft daemon will
own spawning; apps should not spawn plugins directly.

#### 1c. Extract widget reconciliation wrapper

The `DaemonWidgetReconciler` in overview is a thin wrapper over
`waft_ui_gtk::WidgetReconciler`. Extract the pattern so waft-toasts and other
apps can reuse it without copying.

### Phase 2: Central waft daemon

#### 2a. Create waft daemon binary

New workspace member: `crates/waft-daemon/`. Responsibilities:

- Spawn all plugin daemon processes (replaces DaemonSpawner in overview)
- Accept connections from plugins (existing socket protocol)
- Accept connections from apps (new app protocol)
- Route widget updates: plugin → subscribed apps
- Route actions: app → plugin
- Track subscriber counts per plugin

#### 2b. Implement subscriber protocol

Add to waft-ipc:

- `AppMessage::Subscribe`, `AppMessage::Unsubscribe`
- `DaemonToPlugin::SubscriberCount`
- `PluginDaemon::on_subscribers_changed(count: usize)` with default no-op

#### 2c. App registration protocol

Add to waft-ipc:

- `AppMessage::Register { app_id, capabilities }`
- Apps identify themselves so the daemon knows what to route where

#### 2d. Migrate overview to connect via waft daemon

waft-overview stops spawning plugins directly. Instead:

- Connects to waft daemon socket
- Sends Register + Subscribe for all feature plugins
- Receives widget updates from daemon (not directly from plugins)
- Sends actions to daemon (daemon routes to plugin)

### Phase 3: Notifications daemon migration

#### 3a. Extract notifications D-Bus server to daemon process

The D-Bus server (`dbus/server.rs`) becomes a standalone daemon binary
`waft-notifications-daemon`. It:

- Owns org.freedesktop.Notifications on D-Bus
- Implements Notify, CloseNotification, GetCapabilities, GetServerInformation
- Manages notification state (store, lifecycle, grouping, deprioritization)
- Sends NotificationCard widgets via the plugin protocol
- The debouncer, markup processing, category handling stay in the daemon

The daemon does NOT create any GTK widgets.

#### 3b. Add NotificationCard to widget protocol

Add `Widget::NotificationCard` to waft-ipc with fields from Option A above.
Add `NotificationAction { key: String, label: String }` type.

#### 3c. Create waft-toasts app

New workspace member: `crates/waft-toasts/`. A standalone GTK4 layer-shell app:

- Connects to waft daemon, subscribes to notifications plugin
- Receives NotificationCard widgets
- Renders as toast cards with countdown bar, slide animation, auto-dismiss
- Manages toast lifecycle (appearing/visible/hiding/hidden)
- Handles hover-pause, slot limiting, DnD state
- Session lock awareness (pause when locked)

Reuses from extracted infrastructure:

- waft-plugin-client for daemon connection
- waft-ui-gtk for icon rendering
- waft-core for store pattern

The toast window, toast list, countdown bar, and deferred removal patterns move
here from the current notifications plugin UI code.

#### 3d. Notification panel in overview via widget protocol

waft-overview renders NotificationCard widgets in its notification panel. The
grouping, expand/collapse, and panel-specific rendering live in overview. The
DnD toggle becomes a regular daemon widget (FeatureToggle from the notifications
daemon).

#### 3e. Migrate remaining store/lifecycle logic

The notification store reducer, lifecycle state machine (Appearing → Visible →
Hiding → Hidden), and tick-based animation timing need to be split:

- Notification data lifecycle (ingress, replace, close) → notifications daemon
- Toast display lifecycle (appear animation, TTL countdown, dismiss animation)
  → waft-toasts app
- Panel display lifecycle (group management, expand/collapse) → waft-overview

### Phase 4: Sunsetr migration

#### 4a. Migrate sunsetr to daemon

The last remaining cdylib. Sunsetr is a CLI integration — relatively
straightforward daemon migration compared to notifications.

### Phase 5: Remove cdylib plugin system

#### 5a. Remove waft-plugin-api crate

Once all 3 cdylib plugins are migrated to daemons:

- Delete `crates/plugin-api/` (OverviewPlugin trait, loader, export macros)
- Remove `libloading` dependency from overview
- Remove `unsafe-assume-initialized` gtk4 feature from all plugin Cargo.tomls
- Remove cdylib plugin discovery/loading from overview app.rs
- Remove plugin_registry.rs (replaced by waft-plugin-client)

#### 5b. Remove runtime bridge workarounds

Delete `runtime_bridge.rs` from notifications and any other cdylib-specific
tokio runtime workarounds. Daemon plugins use their own tokio runtime natively.

#### 5c. Clean up overview

With all plugins as daemons connecting via waft daemon:

- Remove DaemonSpawner from overview (daemon manages this)
- Remove direct plugin socket connections (goes through waft daemon)
- overview becomes a pure UI app: connect to daemon, render widgets, send actions

### Code deduplication

#### D1. Shared layer-shell app bootstrap

Both waft-overview and waft-toasts need layer-shell window setup, GTK4 init,
config loading, daemon connection. Extract a shared bootstrap module or crate
(`waft-app-shell` or similar) providing:

- GTK4 + layer-shell initialization
- Config loading
- Daemon connection setup
- Session lock detection
- IPC command server (show/hide/toggle)

#### D2. Shared notification rendering utilities

Both waft-toasts and waft-overview render NotificationCard widgets. Share:

- Pango markup sanitization (notification_markup.rs)
- Icon resolution with fallback (app icon lookup)
- Action button generation
- Urgency-based CSS class selection

These go in waft-ui-gtk or a new waft-notification-ui crate.

#### D3. Consolidate store patterns

The PluginStore pattern (waft-core) and notification store share the same
reducer architecture. Ensure daemon plugins use PluginStore consistently,
and app-side state (toast lifecycle, panel grouping) uses the same pattern.

# 6. Syncthing plugin

Provides overlay feature toggle, that enables/pauses user's Syncthing.

# Notification sounds

Play a sound when a notification pops up
Configure sounds=disabled/enabled
Configure sound based on urgency
Configure sound based on notification matching
Sounds are off in Do Not Disturb mode

# Tethering

Add to networkmanager plugin?

Whenever tethering device is detected, display it as a feature toggle

# Auxiliary notification group splits

Sometimes apps have workspaces. It would be useful to split notifications to groups per app workspace. We should investigate if there is a generic way to achieve this. Good example is Slack. Running multiple workspaces seems to be prefixing the notification title with `[{workspace_name}]` and that could be used to group notifications more productively. The Workspace name (if detected) MUST appear in thenotification group header. Optionally we can even load the workspace icon and display it in the notification group header as a secondary icon to provide more visual hints.

# Plugins to implement

**Needs developer clarification:**

- SNI - What is SNI in this context? Server Name Indication? Social Network Integration? Please specify requirements.

# NetworkManager plugin enhancements

### WiFi: Support connecting to new (unsaved) networks with password prompt

**Status:** Requires implementation - significant work needed

**Current limitation:**

- WiFi menu only shows networks with saved connection profiles (`wifi_adapter_widget.rs:214-220`)
- Networks are filtered: `let profiles = dbus::get_connections_for_ssid(&dbus, &ap.ssid).await?;`
- If `profiles.is_empty()`, the network is excluded from the menu

**What needs to be built:**

1. **D-Bus connection creation** (`dbus.rs`):

   - Add `create_wireless_connection()` function to create NM connection profiles dynamically
   - Use D-Bus `AddAndActivateConnection()` method on Settings interface
   - Handle WPA2/WPA3 security types and credentials
   - Current `activate_connection()` (line 390) requires existing connection_path

2. **Password dialog UI** (new file or widget):

   - Create GTK dialog for password entry
   - Support different security types (WPA2-PSK, WPA3-SAE, etc.)
   - Show network name (SSID) in dialog
   - Optional "Save this network" checkbox

3. **WiFi menu updates** (`wifi_menu.rs` and `wifi_adapter_widget.rs`):
   - Show ALL networks (remove filter at line 214-220)
   - Add visual indicator for unsaved vs saved networks (lock icon?)
   - Handle `WiFiMenuOutput::Connect(ssid)` differently for unsaved networks
   - Trigger password dialog when connecting to unsaved network

**Files to modify:**

- `src/features/networkmanager/dbus.rs` - Add D-Bus connection creation
- `src/features/networkmanager/wifi_adapter_widget.rs` - Remove filter, add password dialog logic
- `src/features/networkmanager/wifi_menu.rs` - Add visual indicators for unsaved networks
- New file: `src/features/networkmanager/wifi_password_dialog.rs` (or similar)

**Complexity:** Medium-High (D-Bus API knowledge required, security handling)

# Notification toast bubbles

**Status:** Feature idea - needs design approval

**Concept:** Replace traditional toast notifications with bubble-style notifications like Civilization VI.

**Needs developer input:**

- Visual design mockup or reference
- Animation behavior specification
- Interaction model (click to dismiss, auto-fade, etc.)
- How this integrates with task #8 (positioning)

**Questions to answer:**

- Should all notifications use bubbles, or only certain types?
- Where do bubbles appear (corners, edges, center)?
- How do multiple bubbles stack or cluster?

# Notification toast window position

**Status:** Feature request - can be implemented

**Current:** Notifications appear at top (assumed based on typical behavior)

**Requested:** Support bottom position (and potentially other positions)

**Implementation considerations:**

- Add position configuration (top, bottom, top-left, top-right, bottom-left, bottom-right)
- Fix toast ordering when position changes:
  - Top position: newest on top (stack grows downward)
  - Bottom position: newest on bottom (stack grows upward)
- Update animations to respect position:
  - Slide-in direction should match position
  - Exit animations should feel natural
- Consider interaction with task #7 (bubble style) if both are implemented

**Files to investigate:**

- Notification toast window implementation
- Animation/transition code
- Configuration/settings storage

**Needs developer input:**

- Should this be user-configurable or hardcoded?
- Which positions should be supported initially?

# Simplify clock plugin

It does not need external ping
