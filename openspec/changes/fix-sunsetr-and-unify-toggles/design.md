## Context

Task 1 shows classic glib busy-polling: `spawn_start`/`spawn_stop` call async tokio functions from `glib::spawn_future_local`. The strace shows thousands of zero-timeout ppoll calls per second.

Tasks 2-4 involve sunsetr UX improvements: localized labels, correct state representation, and preset menu support.

Task 3 requires architectural change: merge two separate toggle components into one with CSS-based differentiation.

## Goals / Non-Goals

**Goals:**
- Fix application hang (Task 1)
- Correct sunsetr state representation (Task 1)
- Add localized period labels (Task 2)
- Add expandable preset menu (Task 4)
- Unify toggle components (Task 3)

**Non-Goals:**
- Changing other plugins
- Modifying sunsetr CLI behavior
- Adding new sunsetr features beyond presets

## Decisions

### Decision 1: Move tokio work to tokio runtime

**Choice:** Spawn `spawn_start` and `spawn_stop` on tokio runtime, communicate results via flume channel.

**Rationale:**
- Follows AGENTS.md pattern for runtime bridging
- `spawn_start`/`spawn_stop` use `tokio::process::Command` - must run on tokio runtime
- flume is executor-agnostic, perfect for glib ↔ tokio bridge
- Already used successfully in notifications plugin

**Implementation:**
```rust
// In mod.rs toggle handler:
let ipc_sender = ipc_sender.clone();
tokio::spawn(async move {
    if let Err(e) = spawn_start(ipc_sender).await {
        warn!("[sunsetr] spawn_start failed: {e}");
    }
});
```

**Alternatives considered:**
- Keep in glib context → NO: causes busy-polling
- Use different async runtime → NO: project uses tokio

### Decision 2: State = process running, not period

**Choice:** Toggle active state represents whether sunsetr process is running, regardless of current period (day/night).

**Rationale:**
- User intent: "is night light enabled?" = "is sunsetr running?"
- Current behavior confusing: shows "off" during day even though process is active
- Clicking "off" toggle should start sunsetr if not running, not change period

**Alternatives considered:**
- State = current period → NO: confusing UX, already rejected by user

### Decision 3: CSS-based toggle variants

**Choice:** Single `FeatureToggle` component renders both main and expand buttons always, use CSS class "expandable" to show/hide expand button.

**Rationale:**
- Simpler: one component instead of two
- Consistent structure: `<Box><MainButton /><ExpandButton /></Box>`
- Dynamic: can switch between variants at runtime via CSS class
- No widget rebuilding needed

**Structure:**
```rust
Box {
    css_classes: if expandable { vec!["feature-toggle", "expandable"] } else { vec!["feature-toggle"] },
    MainButton { /* always present */ },
    ExpandButton { /* always present, hidden via CSS if not expandable */ },
}
```

**CSS:**
```css
.feature-toggle:not(.expandable) .expand-button {
    display: none;
}
```

**Alternatives considered:**
- Two separate components → NO: current state, causes duplication
- Conditional widget creation → NO: requires rebuilding on state change

### Decision 4: Preset menu from sunsetr CLI

**Choice:** Query sunsetr presets via `sunsetr preset list --json`, populate menu dynamically.

**Rationale:**
- sunsetr already supports preset management
- JSON output makes parsing easy
- Menu populated on expand, not at init (lazy loading)

**Alternatives considered:**
- Hardcoded presets → NO: not flexible
- Config file → NO: sunsetr already has this

## Risks / Trade-offs

**[Risk] Preset query adds latency** → Acceptable. Menu expand is rare user action; lazy load is appropriate.

**[Risk] CSS selector complexity** → Mitigated. Single `:not(.expandable)` selector is simple.

**[Trade-off] Always render both buttons** → Worth it. Memory cost is minimal, eliminates widget rebuilding.
