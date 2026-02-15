# Notification Pattern Matching and Filtering Design

**Date:** 2026-02-15
**Status:** Approved
**Architecture:** Entity-Based (Approach 3)

## Overview

This design introduces a two-layer notification filtering system: **Notification Groups** (pattern-based categorization) and **Profiles** (rule sets that define actions for each group). The system is fully integrated with the entity-based architecture, making configuration observable and live-updating.

## Goals

1. Allow users to categorize notifications using flexible pattern matching
2. Enable profile-based filtering rules (hide, suppress toast, suppress sound)
3. Replace hardcoded deprioritization logic with user-configurable system
4. Align with entity-based architecture (config as entities)
5. Provide rich Settings UI for managing groups and profiles

## Non-Goals

- Automatic profile switching (time-based, workspace-based)
- Notification persistence to disk
- Statistics/analytics on notification patterns

## Architecture

### Two-Layer System

**Layer 1: Notification Groups**
- Pattern-based categorization of notifications
- Always active (match incoming notifications)
- First match wins (groups evaluated in order)
- Support complex boolean logic with nested AND/OR combinators

**Layer 2: Profiles**
- User-selectable rule sets (e.g., "Work", "Home", "Focus")
- Define actions (hide/no-toast/no-sound) for each group
- One active profile at a time
- Manual switching via Settings UI

**Flow:**
1. Notification arrives → Match against groups → Determine group (or "uncategorized")
2. Look up active profile's rules for matched group
3. Apply actions: hide (drop), suppress toast, suppress sound

---

## Section 1: Entity Types & Schema

### 1.1 Entity: `notification-group`

Represents a named pattern-based group that categorizes notifications.

**URN format:** `notifications/notification-group/{group-id}`

**Entity schema:**
```rust
pub struct NotificationGroup {
    pub id: String,              // e.g., "team-chats", "music-apps"
    pub name: String,            // Display name: "Team Chats", "Music Apps"
    pub order: u32,              // Evaluation order (lower = higher priority)
    pub matcher: RuleCombinator, // Root combinator
}

pub struct RuleCombinator {
    pub operator: CombinatorOperator,
    pub children: Vec<RuleNode>,
}

pub enum CombinatorOperator {
    And,  // All children must match
    Or,   // At least one child must match
}

pub enum RuleNode {
    Pattern(Pattern),           // Leaf: actual pattern match
    Combinator(RuleCombinator), // Branch: nested combinator
}

pub struct Pattern {
    pub field: MatchField,     // What to match against
    pub operator: MatchOperator,
    pub value: String,         // Pattern value (text, regex, etc.)
}

pub enum MatchField {
    AppName,
    AppId,        // desktop entry
    Title,
    Body,
    Category,     // D-Bus category string
    Urgency,      // "low", "normal", "critical"
    Workspace,
}

pub enum MatchOperator {
    Equals,
    NotEquals,
    Contains,
    NotContains,
    StartsWith,
    NotStartsWith,
    EndsWith,
    NotEndsWith,
    MatchesRegex,
    NotMatchesRegex,
}
```

### 1.2 Entity: `notification-profile`

Represents a profile with rules for each group.

**URN format:** `notifications/notification-profile/{profile-id}`

**Entity schema:**
```rust
pub struct NotificationProfile {
    pub id: String,              // e.g., "work", "home", "focus"
    pub name: String,            // Display name: "Work", "Home", "Focus Mode"
    pub rules: HashMap<String, GroupRule>, // group-id -> rule
}

pub struct GroupRule {
    pub hide: RuleValue,         // on/off/default
    pub no_toast: RuleValue,     // on/off/default
    pub no_sound: RuleValue,     // on/off/default
}

pub enum RuleValue {
    On,
    Off,
    Default,  // Inherit from default behavior
}
```

### 1.3 Entity: `active-profile`

Single entity tracking which profile is currently active.

**URN format:** `notifications/active-profile/current`

**Entity schema:**
```rust
pub struct ActiveProfile {
    pub profile_id: String,  // ID of currently active profile
}
```

**Actions:**
- `set-profile` - Switch to a different profile
  - Params: `{"profile_id": "work"}`

---

## Section 2: Pattern Matching Engine with Combinators

### 2.1 Matching Algorithm

**Flow:**
1. Load all `notification-group` entities, sort by `order` (ascending)
2. For each group (in order):
   - Evaluate root combinator against notification
   - If combinator matches → notification belongs to this group (first match wins)
   - If combinator doesn't match → skip to next group
3. If no groups match → notification is "uncategorized" (uses default behavior)

### 2.2 Combinator Evaluation

**Recursive evaluation:**
```rust
fn evaluate_combinator(combinator: &RuleCombinator, notification: &Notification) -> bool {
    match combinator.operator {
        CombinatorOperator::And => {
            // All children must match
            combinator.children.iter().all(|child| evaluate_node(child, notification))
        }
        CombinatorOperator::Or => {
            // At least one child must match
            combinator.children.iter().any(|child| evaluate_node(child, notification))
        }
    }
}

fn evaluate_node(node: &RuleNode, notification: &Notification) -> bool {
    match node {
        RuleNode::Pattern(p) => evaluate_pattern(p, notification),
        RuleNode::Combinator(c) => evaluate_combinator(c, notification),
    }
}
```

### 2.3 Operator Implementation

**Text operators** (case-insensitive):
- `Equals` / `NotEquals`: Exact string match
- `Contains` / `NotContains`: Substring search
- `StartsWith` / `NotStartsWith`: Prefix match
- `EndsWith` / `NotEndsWith`: Suffix match
- `MatchesRegex` / `NotMatchesRegex`: Regex match using `regex` crate

**Special fields:**
- `Category`: Match against parsed `NotificationCategory` enum string representation
- `Urgency`: Match against "low", "normal", "critical"
- `Workspace`: Match against extracted workspace string (if present)

### 2.4 Example: Complex Matching

**Group "Work Notifications":**
```
AND {
  children: [
    Pattern(app_name contains "slack"),
    OR {
      children: [
        Pattern(urgency equals "critical"),
        Pattern(title contains "meeting"),
        Pattern(body contains "@channel")
      ]
    }
  ]
}
```

Matches: `app_name contains "slack" AND (urgency equals "critical" OR title contains "meeting" OR body contains "@channel")`

### 2.5 Pattern Matcher Rebuild

When group entities change:
1. Plugin receives `EntityUpdated` for `notification-group`
2. Rebuild internal `Vec<CompiledGroup>` with pre-compiled regexes
3. Cache compiled matchers for performance (regex compilation is expensive)

**Compiled representation:**
```rust
struct CompiledGroup {
    id: String,
    name: String,
    order: u32,
    matcher: CompiledCombinator,
}

struct CompiledCombinator {
    operator: CombinatorOperator,
    children: Vec<CompiledNode>,
}

enum CompiledNode {
    Pattern(CompiledPattern),
    Combinator(Box<CompiledCombinator>),
}

struct CompiledPattern {
    field: MatchField,
    operator: MatchOperator,
    value: String,
    regex: Option<regex::Regex>,  // Pre-compiled for regex operators
}
```

---

## Section 3: Configuration Persistence (Bidirectional Sync)

The configuration lives in both TOML (persistent storage) and entities (runtime representation). Changes flow in both directions.

### 3.1 Storage Location

**Primary storage:** `~/.config/waft/config.toml`

**Structure:**
```toml
[[plugins]]
id = "plugin::notifications"

# Notification groups
[[plugins.groups]]
id = "team-chats"
name = "Team Chats"
order = 1

[plugins.groups.matcher]
operator = "and"

[[plugins.groups.matcher.children]]
type = "pattern"
field = "app_name"
operator = "contains"
value = "slack"

[[plugins.groups.matcher.children]]
type = "combinator"
operator = "or"

[[plugins.groups.matcher.children.children]]
type = "pattern"
field = "urgency"
operator = "equals"
value = "critical"

[[plugins.groups.matcher.children.children]]
type = "pattern"
field = "title"
operator = "contains"
value = "meeting"

# Profiles
[[plugins.profiles]]
id = "work"
name = "Work"

[plugins.profiles.rules.team-chats]
hide = "off"
no_toast = "off"
no_sound = "off"

[plugins.profiles.rules.music-apps]
hide = "on"
no_toast = "default"
no_sound = "default"
```

**Active profile state:** `~/.local/state/waft/notification-profile` (plain text file with profile ID)

### 3.2 Startup: TOML → Entities

**On plugin initialization:**
1. Load config from `~/.config/waft/config.toml`
2. Parse groups → create `notification-group` entities
3. Parse profiles → create `notification-profile` entities
4. Load active profile from state file → create `active-profile` entity
5. Return entities via `get_entities()`
6. Build internal compiled matcher cache

### 3.3 Runtime: Entity Actions → TOML

**When settings app modifies configuration:**

**Action: `create-group`** (on any entity, e.g., `notifications/active-profile/current`)
- Params: Full `NotificationGroup` JSON
- Plugin: Add to in-memory groups map
- Plugin: Rebuild TOML, write to disk
- Plugin: Send `EntityUpdated` for new `notification-group` entity

**Action: `update-group`** (on `notifications/notification-group/{id}`)
- Params: Updated `NotificationGroup` JSON
- Plugin: Update in-memory groups map
- Plugin: Rebuild TOML, write to disk
- Plugin: Send `EntityUpdated` for modified entity

**Action: `delete-group`** (on `notifications/notification-group/{id}`)
- Plugin: Remove from in-memory groups map
- Plugin: Remove from all profiles' rules
- Plugin: Rebuild TOML, write to disk
- Plugin: Send `EntityRemoved` for entity

**Similar actions for profiles:**
- `create-profile`
- `update-profile`
- `delete-profile`

**Action: `set-profile`** (on `notifications/active-profile/current`)
- Params: `{"profile_id": "work"}`
- Plugin: Update in-memory active profile
- Plugin: Write to state file
- Plugin: Send `EntityUpdated` for `active-profile` entity

### 3.4 TOML Serialization

**Helper function:**
```rust
fn rebuild_toml(groups: &[NotificationGroup], profiles: &[NotificationProfile]) -> Result<String> {
    // Serialize groups + profiles to TOML
    // Merge with existing plugin config (preserve other settings like sounds)
    // Write atomically to config file
}
```

**Atomic write strategy:**
1. Serialize to temp file: `~/.config/waft/config.toml.tmp`
2. `fsync()` to ensure durability
3. Rename to `~/.config/waft/config.toml` (atomic on Unix)

### 3.5 Error Handling

**TOML write failures:**
- Log error: `[notifications/config] Failed to write config: {error}`
- Keep in-memory state consistent (entities remain updated)
- Settings UI shows error notification via toast

**TOML parse failures on startup:**
- Log error: `[notifications/config] Invalid config, using defaults`
- Return empty groups/profiles (fallback to default behavior)
- Optionally create backup: `config.toml.broken`

---

## Section 4: Filtering Logic

This section describes how incoming notifications are filtered based on the active profile and matched groups.

### 4.1 Notification Ingress Flow

**Updated flow in `bin/waft-notifications.rs` ingress monitor:**

```rust
// In the ingress monitor task (existing code path)
IngressEvent::Notify { notification } => {
    // 1. Match notification against groups
    let matched_group = match_notification_to_group(&notification, &groups);

    // 2. Look up active profile's rule for this group
    let rule = get_rule_for_group(&matched_group, &active_profile, &profiles);

    // 3. Evaluate actions
    let actions = evaluate_actions(&rule);

    // 4. Apply filtering
    if actions.hide {
        // Drop notification entirely - don't add to state
        log::debug!("[notifications] Hiding notification from {:?} (group: {:?})",
                    notification.app_name, matched_group);
        continue;
    }

    // 5. Evaluate sound policy (check no_sound rule)
    let sound_decision = if actions.no_sound {
        SoundDecision::Suppress
    } else {
        // Existing sound policy evaluation
        ingress_sound_policy.evaluate(&ctx)
    };

    // 6. Mutate state (existing logic)
    {
        let mut guard = ingress_state.lock()?;
        process_op(&mut guard, NotificationOp::Ingress(notification));
    }

    // 7. Play sound if not suppressed
    if let SoundDecision::Play(sound_id) = sound_decision {
        tokio::spawn(async move { player.play(&sound_id).await });
    }

    // 8. Notify daemon (creates entities with metadata)
    ingress_notifier.notify();
}
```

### 4.2 Action Evaluation

```rust
struct FilterActions {
    hide: bool,       // Drop notification completely
    no_toast: bool,   // Suppress toast (still in panel)
    no_sound: bool,   // Suppress sound
}

fn evaluate_actions(rule: &Option<GroupRule>) -> FilterActions {
    let Some(rule) = rule else {
        // No rule for this group = use defaults
        return FilterActions {
            hide: false,
            no_toast: false,
            no_sound: false,
        };
    };

    FilterActions {
        hide: rule.hide == RuleValue::On,
        no_toast: rule.no_toast == RuleValue::On,
        no_sound: rule.no_sound == RuleValue::On,
    }
}
```

### 4.3 Toast Suppression Mechanism

**Add optional field to `proto::Notification`:**
```rust
pub struct Notification {
    // ... existing fields ...
    pub suppress_toast: bool,
}
```

- Overview checks `suppress_toast` field when deciding whether to show toast
- Panel always shows notification (unless hidden entirely via `hide=on`)

### 4.4 Panel vs Toast Behavior

| Rule       | Panel | Toast | Sound |
|------------|-------|-------|-------|
| hide=on    | ❌    | ❌    | ❌    |
| no-toast=on| ✅    | ❌    | ✅*   |
| no-sound=on| ✅    | ✅    | ❌    |
| All default| ✅    | ✅    | ✅    |

*Unless `no-sound=on` is also set

### 4.5 State Management

**Plugin internal state:**
```rust
struct NotificationsPluginState {
    // Existing notification state
    notifications: HashMap<u64, Notification>,
    panel_notifications: IndexMap<u64, ItemLifecycle>,
    dnd: bool,

    // New: filtering configuration
    groups: Vec<NotificationGroup>,        // Loaded from entities
    profiles: Vec<NotificationProfile>,    // Loaded from entities
    active_profile_id: String,             // Loaded from entity

    // Cached compiled matchers
    compiled_matchers: Vec<CompiledGroup>,
}
```

**State updates on entity changes:**
- `EntityUpdated` for `notification-group` → rebuild `groups` + `compiled_matchers`
- `EntityUpdated` for `notification-profile` → rebuild `profiles`
- `EntityUpdated` for `active-profile` → update `active_profile_id`

---

## Section 5: Settings UI

The settings UI allows users to manage notification groups, profiles, and switch the active profile.

### 5.1 Navigation Structure

**Add new page to `crates/settings/src/window.rs`:**

```rust
let notifications_page = NotificationsPage::new(entity_store, action_callback);
let notifications_clamp = adw::Clamp::builder()
    .maximum_size(600)
    .child(&notifications_page.root)
    .build();

stack.add_named(&notifications_clamp, Some("Notifications"));
```

**Add to sidebar** (`crates/settings/src/sidebar.rs`):
- New category: "Notifications"
- Icon: "preferences-system-notifications-symbolic"

### 5.2 Notifications Page Layout

**Three-section vertical layout:**

```
┌─────────────────────────────────────────┐
│  Active Profile                         │
│  ┌───────────────────────────────────┐  │
│  │ [Dropdown: Work ▼]                │  │
│  └───────────────────────────────────┘  │
├─────────────────────────────────────────┤
│  Notification Groups                    │
│  ┌───────────────────────────────────┐  │
│  │ > Team Chats              [Edit]  │  │
│  │ > Music Apps              [Edit]  │  │
│  │ > System Updates          [Edit]  │  │
│  │                                    │  │
│  │ [+ Add Group]                     │  │
│  └───────────────────────────────────┘  │
├─────────────────────────────────────────┤
│  Profiles                               │
│  ┌───────────────────────────────────┐  │
│  │ > Work                    [Edit]  │  │
│  │ > Home                    [Edit]  │  │
│  │ > Focus Mode              [Edit]  │  │
│  │                                    │  │
│  │ [+ Add Profile]                   │  │
│  └───────────────────────────────────┘  │
└─────────────────────────────────────────┘
```

### 5.3 Component Hierarchy

**Smart container:** `NotificationsPage`
- Subscribes to `notification-group`, `notification-profile`, `active-profile` entities
- Manages state and reconciliation
- Creates child widgets

**Dumb widgets:**

1. **`ActiveProfileSelector`**
   - `adw::ComboRow` with profile list
   - Emits `Output::ProfileSelected(profile_id)`
   - Props: `profiles: Vec<(id, name)>`, `active_id: String`

2. **`GroupsList`**
   - `gtk::ListBox` with `GroupRow` children
   - Emits `Output::EditGroup(id)`, `Output::DeleteGroup(id)`, `Output::AddGroup`
   - Props: `groups: Vec<NotificationGroup>`

3. **`GroupRow`** (adw::ActionRow)
   - Shows group name, order badge
   - Edit button → opens `GroupEditor` dialog
   - Delete button → confirmation dialog

4. **`ProfilesList`**
   - `gtk::ListBox` with `ProfileRow` children
   - Emits `Output::EditProfile(id)`, `Output::DeleteProfile(id)`, `Output::AddProfile`
   - Props: `profiles: Vec<NotificationProfile>`

5. **`ProfileRow`** (adw::ActionRow)
   - Shows profile name, active badge (if current)
   - Edit button → opens `ProfileEditor` dialog

### 5.4 Group Editor Dialog

**Modal dialog for creating/editing groups:**

```
┌─────────────────────────────────────────┐
│  Edit Group: Team Chats                 │
├─────────────────────────────────────────┤
│  Name:     [Team Chats_______________]  │
│  Order:    [1________________________]  │
│                                         │
│  Patterns                               │
│  ┌───────────────────────────────────┐  │
│  │ Combinator: [AND ▼]              │  │
│  │                                   │  │
│  │  Pattern:                         │  │
│  │    Field:    [App Name ▼]        │  │
│  │    Operator: [Contains ▼]        │  │
│  │    Value:    [slack___________]  │  │
│  │    [Remove]                       │  │
│  │                                   │  │
│  │  Combinator: [OR ▼]              │  │
│  │    Pattern:                       │  │
│  │      Field:    [Urgency ▼]       │  │
│  │      Operator: [Equals ▼]        │  │
│  │      Value:    [critical______]  │  │
│  │      [Remove]                     │  │
│  │    [+ Add Pattern]                │  │
│  │    [Remove Combinator]            │  │
│  │                                   │  │
│  │  [+ Add Pattern]                  │  │
│  │  [+ Add Combinator]               │  │
│  └───────────────────────────────────┘  │
│                                         │
│  [Cancel]                      [Save]   │
└─────────────────────────────────────────┘
```

**Nested combinator UI:**
- Indentation to show nesting depth
- Each combinator is an `adw::PreferencesGroup` with expander
- Patterns within combinator are `adw::ActionRow` entries
- Drag handles for reordering (future enhancement)

### 5.5 Profile Editor Dialog

**Modal dialog for creating/editing profiles:**

```
┌─────────────────────────────────────────┐
│  Edit Profile: Work                     │
├─────────────────────────────────────────┤
│  Name: [Work_________________________]  │
│                                         │
│  Rules for Groups                       │
│  ┌───────────────────────────────────┐  │
│  │ Team Chats                        │  │
│  │   Hide:     [Default ▼]          │  │
│  │   No Toast: [Default ▼]          │  │
│  │   No Sound: [Default ▼]          │  │
│  │                                   │  │
│  │ Music Apps                        │  │
│  │   Hide:     [On ▼]               │  │
│  │   No Toast: [Default ▼]          │  │
│  │   No Sound: [Default ▼]          │  │
│  │                                   │  │
│  │ System Updates                    │  │
│  │   Hide:     [Off ▼]              │  │
│  │   No Toast: [On ▼]               │  │
│  │   No Sound: [On ▼]               │  │
│  └───────────────────────────────────┘  │
│                                         │
│  [Cancel]                      [Save]   │
└─────────────────────────────────────────┘
```

**Rules display:**
- One `adw::ExpanderRow` per group
- Three `adw::ComboRow` children for hide/no-toast/no-sound
- Only shows groups that exist (dynamic list)

### 5.6 Actions Flow

**Profile switching:**
1. User selects profile from dropdown
2. `ActiveProfileSelector` emits `Output::ProfileSelected("work")`
3. `NotificationsPage` sends action: `TriggerAction(notifications/active-profile/current, "set-profile", {"profile_id": "work"})`
4. Plugin updates active profile entity
5. Page receives `EntityUpdated`, updates UI

**Creating/editing group:**
1. User clicks "Add Group" or "Edit" on group row
2. `GroupEditor` dialog opens (modal)
3. User edits fields, clicks "Save"
4. Dialog emits `Output::Save(NotificationGroup)`
5. Page sends action: `create-group` or `update-group` with full entity JSON
6. Plugin updates entity, writes to TOML
7. Page receives `EntityUpdated`, rebuilds groups list

**Creating/editing profile:**
1. Similar flow to groups
2. Action: `create-profile` or `update-profile`

---

## Section 6: Migration Strategy

This section covers removing the old deprioritization logic and providing a smooth transition to the new pattern-based system.

### 6.1 Code Removal

**Files to delete:**
- `plugins/notifications/src/store/deprioritize.rs` (entire file)

**Code to remove:**
- `plugins/notifications/bin/waft-notifications.rs`: Remove deprioritization imports and calls
- `plugins/notifications/src/store/manager.rs`: Remove deprioritization integration

**Tests to remove:**
- All tests in `deprioritize.rs`
- Integration tests that relied on hardcoded rules

### 6.2 Default Configuration

**Ship with sensible defaults in documentation:**

Example `~/.config/waft/config.toml`:

```toml
[[plugins]]
id = "plugin::notifications"

# Default groups (user can modify/delete)
[[plugins.groups]]
id = "screenshot-apps"
name = "Screenshot Apps"
order = 1

[plugins.groups.matcher]
operator = "or"

[[plugins.groups.matcher.children]]
type = "pattern"
field = "app_name"
operator = "contains"
value = "screenshot"

[[plugins.groups.matcher.children]]
type = "pattern"
field = "app_name"
operator = "equals"
value = "flameshot"

[[plugins.groups]]
id = "power-apps"
name = "Power & Battery"
order = 2

[plugins.groups.matcher]
operator = "or"

[[plugins.groups.matcher.children]]
type = "pattern"
field = "app_name"
operator = "contains"
value = "power"

[[plugins.groups.matcher.children]]
type = "pattern"
field = "app_name"
operator = "contains"
value = "battery"

# Default profile
[[plugins.profiles]]
id = "default"
name = "Default"

[plugins.profiles.rules.screenshot-apps]
hide = "off"
no_toast = "off"
no_sound = "off"

[plugins.profiles.rules.power-apps]
hide = "off"
no_toast = "on"
no_sound = "off"
```

**Active profile state file:**
`~/.local/state/waft/notification-profile`:
```
default
```

### 6.3 Backward Compatibility

**No automatic migration needed:**
- Old configs without `groups`/`profiles` sections → plugin starts with empty groups/profiles
- System falls back to default behavior (show all notifications normally)
- Users manually configure via settings UI

**Documentation:**
- README.md update explaining the new filtering system
- Migration guide: "How to recreate old deprioritization rules with groups/profiles"

### 6.4 Rollout Strategy

**Phase 1: Implementation**
1. Implement entity types and TOML serialization
2. Implement pattern matching engine with combinators
3. Implement filtering logic in plugin
4. Add comprehensive unit tests

**Phase 2: UI Implementation**
1. Create NotificationsPage, ActiveProfileSelector, GroupsList, ProfilesList
2. Create GroupEditor dialog (start with flat patterns, add nesting later if complex)
3. Create ProfileEditor dialog
4. Integration tests with settings UI

**Phase 3: Testing & Polish**
1. Manual testing with real notifications
2. Performance testing (regex compilation, pattern matching overhead)
3. Error handling polish (TOML write failures, invalid regex patterns)
4. Documentation

**Phase 4: Deprecation**
1. Remove `deprioritize.rs` and related code
2. Update tests
3. Update CLAUDE.md to reflect new architecture

### 6.5 Performance Considerations

**Pattern matching overhead:**
- Regex compilation happens once on config load (cached)
- Matching runs on every notification (should be <1ms for typical configs)
- Benchmark with 50+ groups to ensure acceptable performance

**TOML write frequency:**
- Only on user config changes (rare)
- Atomic write ensures no data loss
- No performance impact on notification ingress path

**Entity count:**
- Typical setup: 10-20 groups + 3-5 profiles = ~25 entities
- Minimal overhead compared to notification entities

### 6.6 Error Recovery

**Invalid regex patterns:**
- Detect at config load time
- Log error: `[notifications/config] Invalid regex in group '{id}': {error}`
- Skip pattern (treat as non-matching)
- Settings UI should validate regex before saving (show error in dialog)

**Corrupted TOML:**
- On startup failure, log error and use empty config
- Create backup: `config.toml.broken.{timestamp}`
- User can manually fix or start fresh via UI

**State file missing:**
- Default to first profile alphabetically (or "default" if exists)
- Create state file on first profile switch

---

## Implementation Checklist

### Plugin Core
- [ ] Define entity types in `waft-protocol`: `NotificationGroup`, `NotificationProfile`, `ActiveProfile`
- [ ] Implement `RuleCombinator`, `Pattern`, `MatchField`, `MatchOperator` types
- [ ] Implement TOML serialization/deserialization for nested combinators
- [ ] Implement pattern matching engine with combinator evaluation
- [ ] Implement compiled matcher cache with pre-compiled regexes
- [ ] Add `suppress_toast` field to `proto::Notification`
- [ ] Implement entity actions: `create-group`, `update-group`, `delete-group`, `create-profile`, `update-profile`, `delete-profile`, `set-profile`
- [ ] Implement TOML ↔ entities bidirectional sync
- [ ] Implement filtering logic in ingress monitor
- [ ] Implement active profile state persistence (`~/.local/state/waft/notification-profile`)
- [ ] Remove `deprioritize.rs` and all deprioritization code
- [ ] Add comprehensive unit tests for pattern matching
- [ ] Add integration tests for TOML sync

### Settings UI
- [ ] Add "Notifications" page to settings window
- [ ] Add "Notifications" entry to sidebar
- [ ] Implement `NotificationsPage` smart container
- [ ] Implement `ActiveProfileSelector` widget
- [ ] Implement `GroupsList` and `GroupRow` widgets
- [ ] Implement `ProfilesList` and `ProfileRow` widgets
- [ ] Implement `GroupEditor` dialog with combinator UI
- [ ] Implement `ProfileEditor` dialog
- [ ] Add regex validation in `GroupEditor`
- [ ] Add entity subscription and reconciliation logic
- [ ] Add error handling and user feedback (toasts)

### Documentation
- [ ] Update `plugins/notifications/README.md` with filtering system docs
- [ ] Add migration guide from old deprioritization to new system
- [ ] Update `CLAUDE.md` to reflect new architecture
- [ ] Add example configurations to documentation
- [ ] Document TOML configuration format

### Testing & Polish
- [ ] Manual testing with various notification sources
- [ ] Performance benchmarking (pattern matching, TOML writes)
- [ ] Error handling testing (invalid regex, corrupted TOML)
- [ ] UI/UX polish (dialog layouts, error messages)
- [ ] Accessibility review (keyboard navigation, screen readers)

---

## Future Enhancements

**Phase 2 (post-MVP):**
- Test/preview mode: Show which group recent notifications matched
- Drag-and-drop reordering of groups (change evaluation order)
- Import/export profiles
- Notification statistics (count per group, trends)

**Phase 3 (advanced):**
- Automatic profile switching (time-based, workspace-based)
- Conditional actions (e.g., "hide critical notifications only during work hours")
- Pattern templates library (common patterns users can copy)
- Machine learning suggestions ("You often dismiss notifications from X, create a group?")

---

## References

- **Entity-based architecture:** `docs/plans/2024-*-entity-based-architecture.md`
- **Settings app patterns:** `crates/settings/src/pages/bluetooth.rs`, `wifi.rs`
- **Existing config loading:** `plugins/notifications/src/config.rs`
- **Sound policy (similar pattern matching):** `plugins/notifications/src/sound/policy.rs`
