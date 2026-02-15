# Custom Notification Sounds Design

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Allow users to select a custom sound per notification group, and configure the default sound used when no group-specific sound is set.

**Architecture:** Extend `GroupRule` with an optional `sound` field. The SoundPolicy cascade gains a new step between filter-action check and urgency fallback: per-group custom sound. Default sounds (per-urgency) become configurable via a settings UI backed by a new `sound-config` entity type. Sound config persisted to TOML alongside groups/profiles.

**Tech Stack:** Rust, waft-protocol entities, SoundPolicy, SoundPlayer (canberra-gtk-play), GTK4/libadwaita settings UI.

---

## Current Architecture

### Sound Decision Cascade (7 tiers)

```
1. Master toggle disabled → Silent
2. DND active → Silent
3. suppress-sound D-Bus hint → Silent
4. Explicit sound-file D-Bus hint → Play(path)
5. Explicit sound-name D-Bus hint → Play(name)
6. Per-app rules (first match) → Play(sound) or Silent
7. Urgency fallback → Play(urgency_default)
```

### Current Data Model

- **`SoundConfig`** (config.rs): `enabled`, `urgency: UrgencySounds`, `rules: Vec<SoundRule>` — loaded once at startup from TOML, immutable
- **`SoundPolicy`** (policy.rs): Pure decision engine, evaluates `NotificationContext` → `SoundDecision`
- **`GroupRule`** (notification_filter.rs): `hide: RuleValue`, `no_toast: RuleValue`, `no_sound: RuleValue`
- **`FilterActions`** (lib.rs): `hide: bool`, `no_toast: bool`, `no_sound: bool` — resolved from active profile + matched group

### Current Flow

1. D-Bus notification arrives
2. Matcher finds first matching group ID
3. `get_filter_actions(group_id)` → `FilterActions` (from active profile's `GroupRule`)
4. If `filter_actions.no_sound` → `SoundDecision::Silent`
5. Otherwise → `SoundPolicy::evaluate(ctx)` → cascade steps 1-7

### Key Constraint

SoundPolicy is currently **immutable after startup** (`Arc<SoundPolicy>` created from static config). To support runtime-configurable sounds, we need either:
- (A) Make SoundPolicy hot-reloadable (complex, breaks immutability guarantee)
- (B) Move per-group sound into the filter layer, before SoundPolicy is consulted (simple, extends existing pattern)

**Approach B is chosen** — it extends the existing `FilterActions` pattern naturally.

---

## Design

### New Sound Decision Cascade (8 tiers)

```
1. Master toggle disabled → Silent
2. DND active → Silent
3. suppress-sound D-Bus hint → Silent
4. Explicit sound-file D-Bus hint → Play(path)
5. Explicit sound-name D-Bus hint → Play(name)
6. Per-group custom sound (from active profile) → Play(sound)    ← NEW
7. Per-app rules (first match) → Play(sound) or Silent
8. Default sound for urgency (configurable) → Play(default)      ← CONFIGURABLE
```

Step 6 is new: when a notification matches a group and the active profile specifies a custom sound for that group, use it. This takes priority over per-app rules and urgency defaults.

Step 8 changes: urgency defaults become runtime-configurable via the settings UI instead of only via TOML.

### Data Model Changes

#### 1. `GroupRule` — add `sound` field

```rust
// crates/protocol/src/entity/notification_filter.rs
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct GroupRule {
    pub hide: RuleValue,
    pub no_toast: RuleValue,
    pub no_sound: RuleValue,
    #[serde(default)]               // NEW
    pub sound: Option<String>,       // NEW — XDG theme name or file path
}
```

Semantics:
- `sound: None` + `no_sound: Default` → fall through to SoundPolicy (steps 7-8)
- `sound: None` + `no_sound: On` → Silent
- `sound: Some("bell")` + `no_sound: Default|Off` → Play("bell"), overrides steps 7-8
- `sound: Some("bell")` + `no_sound: On` → `no_sound` wins (Silent). The `no_sound` field takes precedence; `sound` is an alternative to default, not an override of suppression.

#### 2. `FilterActions` — add `sound` field

```rust
// plugins/notifications/src/lib.rs
#[derive(Debug, Default)]
pub struct FilterActions {
    pub hide: bool,
    pub no_toast: bool,
    pub no_sound: bool,
    pub sound: Option<String>,   // NEW — custom sound from group rule
}
```

#### 3. `SoundConfigEntity` — new entity type for default sounds

```rust
// crates/protocol/src/entity/notification_filter.rs
pub const SOUND_CONFIG_ENTITY_TYPE: &str = "sound-config";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SoundConfigEntity {
    pub enabled: bool,
    pub default_low: String,
    pub default_normal: String,
    pub default_critical: String,
}
```

This exposes the current `SoundConfig.enabled` + `UrgencySounds` as a daemon entity, allowing the settings UI to read and modify defaults.

### Ingress Flow Change

```rust
// In ingress monitor (waft-notifications.rs):

// After matching group + getting filter actions:
let sound_decision = if filter_actions.no_sound {
    SoundDecision::Silent
} else if let Some(ref custom_sound) = filter_actions.sound {
    // NEW: Per-group custom sound from active profile
    SoundDecision::Play(custom_sound.clone())
} else {
    // Existing: SoundPolicy cascade (steps 1-5, 7-8)
    ingress_sound_policy.evaluate(&ctx)
};
```

### SoundPolicy Reload for Default Sounds

When the settings UI updates default sounds, the plugin must reload `SoundPolicy`:

```rust
// plugins/notifications/src/lib.rs — new field:
sound_policy: Arc<ArcSwap<SoundPolicy>>,

// On "update-sound-config" action:
// 1. Update SoundConfig in memory
// 2. Rebuild SoundPolicy
// 3. Store via arc_swap (no mutex needed for readers)
// 4. Persist to TOML
```

Actually, `ArcSwap` adds a dependency. Simpler approach: wrap `SoundPolicy` in `Arc<StdMutex<SoundPolicy>>`. The policy is only consulted in the ingress monitor (single consumer), so contention is minimal.

Simplest approach: wrap in `Arc<StdMutex<SoundConfig>>`, rebuild `SoundPolicy` on each evaluate call. But that's wasteful.

**Final decision:** Use `Arc<StdMutex<SoundPolicy>>`. The ingress monitor locks it for each notification (fast, single reader), and the plugin's `handle_action` rebuilds it on config change (rare writer).

### Settings UI Changes

#### Default Sounds Section (new)

Add a "Sounds" section to the notifications settings page:

```
┌─ Sounds ──────────────────────────────────┐
│ ┌────────────────────────────────────────┐ │
│ │ [✓] Enable notification sounds         │ │
│ │ Low urgency:     [message-new-instant] │ │
│ │ Normal urgency:  [message-new-email  ] │ │
│ │ Critical urgency:[dialog-warning     ] │ │
│ └────────────────────────────────────────┘ │
└────────────────────────────────────────────┘
```

- Toggle row for master enable/disable
- Entry rows for each urgency default sound (XDG theme names)
- Sends `update-sound-config` action on change

#### Per-Group Sound in Profile Rules

Extend the existing profile group rule UI:

```
┌─ Work Profile ──────────────────────────────┐
│ ▼ Team Chats                    [Remove]    │
│   Hide:           [Default ▼]               │
│   Suppress Toast: [On ▼]                    │
│   Suppress Sound: [Default ▼]               │
│   Custom Sound:   [message-new-instant    ] │ ← NEW
│ ▼ Build Alerts                  [Remove]    │
│   ...                                       │
│ [Add Group ▼] [Add Group]                   │
└─────────────────────────────────────────────┘
```

- Entry row for custom sound (XDG theme name or absolute file path)
- Empty = use default (no custom sound)
- Disabled when "Suppress Sound" is "On"

### TOML Persistence

#### Groups/Profiles (existing + sound field)

```toml
[[plugins.profiles]]
id = "work"
name = "Work"

[plugins.profiles.rules.team-chats]
hide = "off"
no_toast = "on"
no_sound = "default"
sound = "message-new-instant"    # NEW — optional
```

#### Sound Config (existing section, now bidirectional)

```toml
[plugins.sounds]
enabled = true

[plugins.sounds.urgency]
low = "message-new-instant"
normal = "message-new-email"
critical = "dialog-warning"
```

The `toml_sync.rs` writer must now also serialize `SoundConfig` alongside groups/profiles.

### Plugin Actions (new + modified)

| Action | URN | Params | Effect |
|--------|-----|--------|--------|
| `update-sound-config` | `notifications/sound-config/default` | `SoundConfigEntity` JSON | Update SoundPolicy + persist TOML |
| `update-profile` (existing) | `notifications/notification-profile/{id}` | `NotificationProfile` JSON | Now includes `sound` in `GroupRule` |

---

## Implementation Tasks

### Task 1: Extend `GroupRule` with `sound` field

**Files:**
- Modify: `crates/protocol/src/entity/notification_filter.rs`

Add `sound: Option<String>` to `GroupRule` with `#[serde(default)]`. Update existing test to include the new field. This is backward-compatible: existing TOML configs without `sound` will deserialize to `None`.

**Step 1:** Add field to `GroupRule`:
```rust
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct GroupRule {
    pub hide: RuleValue,
    pub no_toast: RuleValue,
    pub no_sound: RuleValue,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sound: Option<String>,
}
```

**Step 2:** Update `serialize_notification_profile` test to include `sound: Some("bell".to_string())`.

**Step 3:** Add test for backward compatibility (deserialize without `sound` field → `None`).

**Step 4:** Run `cargo test -p waft-protocol`.

**Step 5:** Fix all compilation errors in the workspace caused by the new required field. Every place that constructs a `GroupRule` needs `sound: None` added. Check:
- `plugins/notifications/src/config.rs` (TomlProfile parsing)
- `crates/settings/src/notifications/profiles_section.rs` (default GroupRule creation)
- Any test files constructing GroupRule

**Step 6:** Run `cargo build --workspace && cargo test --workspace`.

**Step 7:** Commit.

---

### Task 2: Add `SoundConfigEntity` to protocol

**Files:**
- Modify: `crates/protocol/src/entity/notification_filter.rs`

**Step 1:** Add entity type constant and struct:
```rust
pub const SOUND_CONFIG_ENTITY_TYPE: &str = "sound-config";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SoundConfigEntity {
    pub enabled: bool,
    pub default_low: String,
    pub default_normal: String,
    pub default_critical: String,
}
```

**Step 2:** Add serialization test.

**Step 3:** Run `cargo test -p waft-protocol`.

**Step 4:** Commit.

---

### Task 3: Extend `FilterActions` with `sound` field

**Files:**
- Modify: `plugins/notifications/src/lib.rs`

**Step 1:** Add `sound: Option<String>` to `FilterActions` struct.

**Step 2:** Update `get_filter_actions()` to populate `sound` from `GroupRule::sound`:
```rust
FilterActions {
    hide: rule.hide == filter_proto::RuleValue::On,
    no_toast: rule.no_toast == filter_proto::RuleValue::On,
    no_sound: rule.no_sound == filter_proto::RuleValue::On,
    sound: rule.sound.clone(),
}
```

**Step 3:** Update existing test `get_filter_actions_applies_profile_rules` to verify `sound` field.

**Step 4:** Add test for custom sound propagation.

**Step 5:** Run `cargo test -p waft-plugin-notifications`.

**Step 6:** Commit.

---

### Task 4: Use per-group sound in ingress monitor

**Files:**
- Modify: `plugins/notifications/bin/waft-notifications.rs`

**Step 1:** Update the sound decision logic in ingress monitor:
```rust
let sound_decision = if filter_actions.no_sound {
    SoundDecision::Silent
} else if let Some(ref custom_sound) = filter_actions.sound {
    SoundDecision::Play(custom_sound.clone())
} else {
    // existing SoundPolicy evaluation
    let ctx = NotificationContext { ... };
    ingress_sound_policy.evaluate(&ctx)
};
```

**Step 2:** Run `cargo build -p waft-plugin-notifications`.

**Step 3:** Commit.

---

### Task 5: Make SoundPolicy runtime-reloadable

**Files:**
- Modify: `plugins/notifications/src/lib.rs`
- Modify: `plugins/notifications/bin/waft-notifications.rs`

**Step 1:** Add `sound_config` field to `NotificationsPlugin`:
```rust
sound_config: Arc<StdMutex<SoundConfig>>,
```

**Step 2:** Add `SoundConfig` to constructor params and store it.

**Step 3:** Expose `SoundConfigEntity` in `get_entities()`:
```rust
let sound_cfg = match self.sound_config.lock() { ... };
entities.push(Entity::new(
    Urn::new("notifications", SOUND_CONFIG_ENTITY_TYPE, "default"),
    SOUND_CONFIG_ENTITY_TYPE,
    &SoundConfigEntity {
        enabled: sound_cfg.enabled,
        default_low: sound_cfg.urgency.low.clone(),
        default_normal: sound_cfg.urgency.normal.clone(),
        default_critical: sound_cfg.urgency.critical.clone(),
    },
));
```

**Step 4:** Handle `update-sound-config` action:
```rust
("sound-config", "update-sound-config") => {
    let entity: SoundConfigEntity = serde_json::from_value(params)?;
    let new_config = SoundConfig {
        enabled: entity.enabled,
        urgency: UrgencySounds {
            low: entity.default_low,
            normal: entity.default_normal,
            critical: entity.default_critical,
        },
        rules: existing_rules,  // preserve per-app rules
    };
    // Update in-memory config
    *self.sound_config.lock()... = new_config.clone();
    // Rebuild SoundPolicy
    *self.sound_policy.lock()... = SoundPolicy::new(new_config);
    // Persist to TOML
    self.sync_sound_config_to_toml()?;
}
```

**Step 5:** Add `sound_policy: Arc<StdMutex<SoundPolicy>>` field to plugin, pass to ingress monitor. Change ingress monitor to lock `sound_policy` instead of using a direct `Arc<SoundPolicy>`.

**Step 6:** Register `SOUND_CONFIG_ENTITY_TYPE` in `handle_provides` in `main()`.

**Step 7:** Update `main()` to pass `sound_config` to plugin constructor and share the `sound_policy` mutex.

**Step 8:** Add `sync_sound_config_to_toml()` method — extend `toml_sync.rs` to write sound config.

**Step 9:** Run `cargo build --workspace && cargo test --workspace`.

**Step 10:** Commit.

---

### Task 6: Settings UI — Sound Defaults Section

**Files:**
- Create: `crates/settings/src/notifications/sound_section.rs`
- Modify: `crates/settings/src/notifications/mod.rs`
- Modify: `crates/settings/src/pages/notifications.rs`

**Step 1:** Create `sound_section.rs` — smart container subscribing to `sound-config` entity:

```
adw::PreferencesGroup "Sounds"
  adw::SwitchRow "Enable notification sounds"
  adw::EntryRow "Default sound (low urgency)"
  adw::EntryRow "Default sound (normal urgency)"
  adw::EntryRow "Default sound (critical urgency)"
```

- Reconcile with entity data (update fields on entity change, guard flag pattern)
- On switch/entry change → send `update-sound-config` action with full `SoundConfigEntity`

**Step 2:** Export module in `mod.rs`.

**Step 3:** Add `SoundSection` to `NotificationsPage` between `ActiveProfileSection` and `GroupsSection`.

**Step 4:** Subscribe to `sound-config` entity type in `pages/notifications.rs`.

**Step 5:** Run `cargo build --workspace`.

**Step 6:** Commit.

---

### Task 7: Settings UI — Per-Group Custom Sound in Profiles

**Files:**
- Modify: `crates/settings/src/notifications/profiles_section.rs`

**Step 1:** In `build_group_row()`, add a custom sound entry row after the three rule dropdowns:

```rust
let sound_entry = adw::EntryRow::builder()
    .title("Custom Sound")
    .text(rule.sound.as_deref().unwrap_or(""))
    .show_apply_button(true)  // shows Apply button on change
    .build();
```

**Step 2:** Wire `sound_entry.connect_apply()` — on apply:
- Read entry text
- If empty → set `rule.sound = None`
- If non-empty → set `rule.sound = Some(text)`
- Clone profile, update rule, send `update-profile` action

**Step 3:** When `no_sound` dropdown is set to "On", disable the sound entry row:
```rust
sound_entry.set_sensitive(rule.no_sound != RuleValue::On);
```
Update sensitivity in the `no_sound` dropdown change callback.

**Step 4:** Run `cargo build --workspace`.

**Step 5:** Commit.

---

### Task 8: Persist sound config in TOML sync

**Files:**
- Modify: `plugins/notifications/src/filter/toml_sync.rs`

**Step 1:** Extend `write_filter_config` (or add a new `write_sound_config` function) to write the sound config section:

```rust
pub fn write_sound_config(
    sound_config: &SoundConfig,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    // Same pattern as write_filter_config:
    // Read existing TOML, find notifications plugin entry,
    // serialize SoundConfig into "sounds" table, write back atomically
}
```

**Step 2:** Call from plugin's `sync_sound_config_to_toml()`.

**Step 3:** Add test for round-trip: write sound config → read back → verify.

**Step 4:** Run `cargo test -p waft-plugin-notifications`.

**Step 5:** Commit.

---

### Task 9: Verification

**Step 1:** `cargo build --workspace` — zero warnings.

**Step 2:** `cargo test --workspace` — all pass.

**Step 3:** Runtime verification:
1. Start daemon + settings app → Notifications page
2. Verify "Sounds" section shows with master toggle + urgency entries
3. Toggle sounds off → verify notifications are silent
4. Change normal urgency default → verify new notifications use it
5. Create a group + profile with custom sound → verify that group's notifications play custom sound
6. Set "Suppress Sound: On" on a group → verify custom sound entry is disabled and notifications are silent
7. Remove custom sound (clear entry) → verify fallback to urgency default

---

## Summary of Changes

| Layer | Change | Scope |
|-------|--------|-------|
| Protocol | Add `sound: Option<String>` to `GroupRule` | Additive, backward-compatible |
| Protocol | Add `SoundConfigEntity` + `SOUND_CONFIG_ENTITY_TYPE` | New entity type |
| Plugin | Add `sound` to `FilterActions` | Additive |
| Plugin | Per-group sound in ingress monitor | 3 lines in existing if-else |
| Plugin | Runtime-reloadable `SoundPolicy` | Wrap in `Arc<StdMutex>` |
| Plugin | `update-sound-config` action | New action handler |
| Plugin | Sound config TOML persistence | Extend existing sync |
| Settings | Sound defaults section | New smart container |
| Settings | Custom sound entry per group in profiles | Extend existing `build_group_row` |
