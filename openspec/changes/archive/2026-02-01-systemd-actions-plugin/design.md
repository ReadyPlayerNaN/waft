## Context

**Current State:**
- Sacrebleui provides unified overlay control for system features (audio, brightness, network, etc.)
- System power/session actions (lock, logout, shutdown, reboot, suspend) require external tools or WM shortcuts
- Existing D-Bus infrastructure (`DbusHandle`) wraps `zbus` v5.0 for async operations
- Session module monitors login1.Session signals (Lock/Unlock) but doesn't expose action methods
- Header layout: horizontal box with left-aligned widgets sorted by weight
- MenuStore pattern: centralized single-open menu coordination using broadcast channels

**Constraints:**
- Must integrate with existing plugin architecture (Plugin trait, WidgetRegistrar)
- Must follow GTK4/Relm4 patterns (RefCell, Rc, Arc for state management)
- Must gracefully degrade if systemd/D-Bus unavailable (desktop-agnostic)
- Cannot block GTK main thread (async D-Bus calls via tokio)

**Stakeholders:**
- End users needing quick access to system actions
- Desktop environment integration (PolicyKit for authorization)

## Goals / Non-Goals

**Goals:**
- Provide header widgets for session actions (Lock, Logout) and power actions (Reboot, Shutdown, Suspend)
- Integrate with org.freedesktop.login1.Manager D-Bus interface for system operations
- Use slide-down menu pattern consistent with existing UI (SliderControlWidget, FeatureToggleExpandable)
- Coordinate with MenuStore to ensure single-open menu behavior
- Handle PolicyKit authorization gracefully with user feedback
- Position action buttons in the header (high weight for visual right-alignment)

**Non-Goals:**
- Confirmation dialogs (defer to future iteration - rely on PolicyKit prompts for now)
- Hibernate support (not in initial proposal, can add later)
- Custom icon selection (use system standard icons)
- Header layout restructuring for true right-alignment (use weight-based ordering first)

## Decisions

### Decision 1: Two Separate Widget Instances vs Single Multi-Button Widget

**Chosen:** Two separate `ActionGroupWidget` instances (one for session, one for power)

**Rationale:**
- Follows existing plugin pattern (Clock plugin registers single widget, Audio plugin registers per-device)
- Allows independent menu management with unique menu IDs
- Cleaner separation of concerns (session vs power actions)
- Easier to remove/disable one group independently via plugin config

**Alternatives considered:**
- Single widget with two button groups → More complex menu ID management, harder to configure independently
- Separate plugins for session and power → Overkill, shared D-Bus client would need coordination

### Decision 2: D-Bus Integration Architecture

**Chosen:** New `SystemdDbusClient` wrapping `DbusHandle` with typed action methods

**Rationale:**
- Reuses existing `DbusHandle` infrastructure (consistent with audio, network, brightness plugins)
- Provides type-safe action enum instead of raw D-Bus method names
- Centralizes session path resolution (reuse `SessionMonitor::get_session_path()`)
- Graceful initialization failure (return `None` if D-Bus unavailable, similar to SessionMonitor pattern)

**Alternatives considered:**
- Direct `zbus::Connection` usage → Duplicates existing DbusHandle abstractions
- Extend SessionMonitor → Wrong responsibility (monitor is passive, this is active control)
- Reuse DbusHandle directly without wrapper → Leaks D-Bus details into widget code

**Implementation approach:**
```rust
pub struct SystemdDbusClient {
    dbus: Arc<DbusHandle>,
    session_path: String,
}

pub enum SystemAction {
    LockSession,
    Terminate,
    Reboot { interactive: bool },
    PowerOff { interactive: bool },
    Suspend { interactive: bool },
}

impl SystemdDbusClient {
    pub async fn new(dbus: Arc<DbusHandle>) -> Option<Self> { /* ... */ }
    pub async fn execute_action(&self, action: SystemAction) -> Result<()> { /* ... */ }
}
```

### Decision 3: Widget Structure and Menu Pattern

**Chosen:** Follow `SliderControlWidget` expand button pattern with `gtk::Revealer` menus

**Rationale:**
- Consistent with existing codebase patterns (audio, brightness controls)
- MenuStore subscription handles chevron rotation and revealer visibility
- Revealer provides smooth slide-down animation (RevealerTransitionType::SlideDown)
- Expand button emits `MenuOp::OpenMenu(menu_id)` for coordination

**Structure:**
```
ActionGroupWidget {
  root: gtk::Box (Horizontal)
    ├─ icon_button: gtk::Button (icon + label, non-clickable or future shortcut)
    ├─ expand_button: gtk::Button (chevron)
    └─ menu_revealer: gtk::Revealer
         └─ ActionMenuWidget (vertical box of MenuItemWidget rows)
}
```

**Menu items:** Use `MenuItemWidget` (existing component) for consistent clickable rows with icons

**Alternatives considered:**
- Dropdown/Popover widgets → Not consistent with existing slide-down pattern
- Single button that opens menu on click → Less clear expand affordance
- Menu as separate registered widget → Breaks encapsulation, harder menu coordination

### Decision 4: Header Positioning Strategy

**Chosen:** Use high weights (100+) for natural right-side positioning

**Rationale:**
- Header widgets are appended left-to-right in weight order (main_window.rs:779-783)
- Clock plugin uses weight 10 (early/left)
- High weights (100, 101) will naturally position rightmost
- No modification to main_window.rs or Widget struct needed
- Can revisit if visual spacing is problematic

**Alternatives considered:**
- Add `alignment` field to Widget struct → Invasive change to plugin.rs API
- CSS flexbox right-alignment → Fragile, depends on specific CSS classes
- Modify main_window.rs layout → Unnecessary complexity for first iteration

**Widget weights:**
- Session Actions: weight 100
- Power Actions: weight 101

### Decision 5: PolicyKit Interactive Flag

**Chosen:** Pass `interactive: true` to power action D-Bus methods

**Rationale:**
- Allows PolicyKit to show authentication dialogs when needed
- Better UX than silent failures for unprivileged users
- Consistent with system daemon behavior (systemctl defaults to interactive)
- User expects permission prompts for destructive actions

**Alternatives considered:**
- `interactive: false` → Silent failures, poor UX
- Check permissions first → Racy, doesn't handle PolicyKit temporary authorizations

### Decision 6: Error Handling Strategy

**Chosen:** Graceful degradation at plugin init + user-facing errors at action time

**Initialization (plugin init):**
- If D-Bus connection fails → Log warning, continue without plugin (return None from SystemdDbusClient::new)
- If session path unavailable → Fall back to "/org/freedesktop/login1/session/auto"

**Action execution (button click):**
- D-Bus method errors → Show GTK MessageDialog with error details
- PolicyKit authorization denied → Show user-friendly message ("Permission denied, contact administrator")
- Connection lost → Show "System service unavailable" message

**Rationale:**
- Matches existing plugin patterns (battery, network, bluetooth all gracefully degrade)
- User needs feedback for action failures (unlike passive monitoring)
- Prevents crashes on non-systemd systems (Sacrebleui is desktop-agnostic)

## Risks / Trade-offs

### Risk: PolicyKit Authorization Failures
**Trade-off:** Users without proper PolicyKit rules will see auth prompts or denials

**Mitigation:**
- Use `interactive: true` to allow password entry
- Show clear error messages explaining PolicyKit requirements
- Document required PolicyKit policies in README

### Risk: Session Path Resolution Failure
**Trade-off:** Lock/Logout actions may fail if XDG_SESSION_ID unavailable

**Mitigation:**
- Fall back to `/org/freedesktop/login1/session/auto` (logind resolves to caller's session)
- Reuse battle-tested `SessionMonitor::get_session_path()` logic
- Log warnings for debugging

### Risk: No Confirmation Dialogs for Destructive Actions
**Trade-off:** Users might accidentally trigger shutdown/reboot/logout

**Mitigation:**
- PolicyKit prompts provide some friction (password entry)
- Future iteration can add confirmation dialogs before D-Bus calls
- Document this limitation in proposal/specs

### Risk: Widget Positioning May Not Appear "Right-Aligned"
**Trade-off:** High weights position widgets rightmost but don't guarantee visual right-alignment

**Mitigation:**
- Test visual appearance with other header widgets
- If spacing is wrong, can add CSS flexbox or Widget alignment field in future iteration
- Clock plugin example shows header widgets work acceptably

### Risk: D-Bus Call Blocking
**Trade-off:** Power actions may take 100-200ms, could feel laggy

**Mitigation:**
- All D-Bus calls are async via tokio (doesn't block GTK thread)
- Consider adding spinner/loading state to action buttons (future iteration)
- Most operations complete quickly (<100ms)

## Migration Plan

**Deployment steps:**
1. Add `systemd_actions` module to `src/features/mod.rs`
2. Register `SystemdActionsPlugin` in `src/main.rs` plugin initialization
3. No config changes required (plugin has no required settings)
4. Build and test on development system
5. Release as new plugin (no breaking changes)

**Rollback strategy:**
- Remove plugin registration from main.rs
- Comment out module declaration in features/mod.rs
- No database migrations or persistent state to clean up

**Compatibility:**
- Optional plugin (graceful degradation if D-Bus unavailable)
- No API changes to existing plugins or core
- Works on systemd and non-systemd systems (fails gracefully)

**Testing plan:**
- Manual testing: Click each action, verify D-Bus calls
- Error testing: Deny PolicyKit, disconnect D-Bus, run on non-systemd
- Menu coordination: Verify single-open behavior with other menus

## Open Questions

### Q1: Should we add confirmation dialogs before destructive actions?

**Current decision:** Defer to future iteration

**Considerations:**
- Adds UX friction but prevents accidents
- PolicyKit prompts already provide some confirmation
- Could make confirmation optional via plugin config

**Recommendation:** Start without confirmations, gather user feedback

### Q2: What system icons should we use?

**Proposed:**
- Session group button: `system-users-symbolic`
- Power group button: `system-shutdown-symbolic`
- Lock action: `system-lock-screen-symbolic`
- Logout action: `system-log-out-symbolic`
- Reboot action: `system-reboot-symbolic`
- Shutdown action: `system-shutdown-symbolic`
- Suspend action: `media-playback-pause-symbolic` (or `system-suspend-symbolic` if available)

**Recommendation:** Use standard FreeDesktop icon names, test on target theme

### Q3: Should Hibernate be included?

**Current decision:** Not in initial implementation (not in proposal)

**Considerations:**
- Not all systems support hibernate
- Adds menu clutter
- Can add in future iteration with capability detection

**Recommendation:** Add in v2 if users request it

### Q4: Should we add keyboard shortcuts for actions?

**Current decision:** Not in initial implementation

**Considerations:**
- Useful for power users
- Requires keybinding configuration system
- Outside scope of this change

**Recommendation:** Defer to separate feature request
