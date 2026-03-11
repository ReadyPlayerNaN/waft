# Claude Usage Plugin Design

**Date:** 2026-03-11

## Overview

A zero-config waft plugin that surfaces Claude Code usage data (5-hour and 7-day rate limit utilization) in the waft overview via two `InfoCardWidget`s.

---

## Entity Type

**File:** `crates/protocol/src/entity/ai.rs` (new domain module)

```rust
pub const ENTITY_TYPE: &str = "claude-usage";

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ClaudeUsage {
    pub five_hour_pct: f64,       // 0.0–100.0
    pub five_hour_reset_at: i64,  // Unix ms
    pub seven_day_pct: f64,
    pub seven_day_reset_at: i64,  // Unix ms
}
```

Single entity with URN `claude/claude-usage/me`. Added to `entity::mod.rs` under the `ai` domain.

---

## Plugin

**Directory:** `plugins/claude/`
**Binary:** `waft-claude-daemon`
**No configuration required** — zero-config, reads credentials automatically.

### Credential management

Re-reads `~/.claude/.credentials.json` before every poll cycle. Structure:

```json
{
  "claudeAiOauth": {
    "accessToken": "...",
    "refreshToken": "...",
    "expiresAt": 1234567890000
  }
}
```

The plugin **never refreshes tokens itself.** Anthropic uses refresh token rotation (each refresh invalidates the old refresh token), so any in-memory refresh would corrupt Claude Code's stored credentials. Instead:

- Re-read the file before every 5-minute poll
- If `expiresAt` is in the past, skip the poll cycle, log a warning, and retry in 5 minutes
- Claude Code refreshes the file automatically when in active use (access tokens last 8 hours)

### Polling loop

- Re-reads `~/.claude/.credentials.json` at the start of each cycle
- If file missing or token expired: skip cycle, log warning, retry in 5 minutes
- Polls `GET https://api.anthropic.com/api/oauth/usage` every **5 minutes**
- Headers: `Authorization: Bearer <accessToken>`, `anthropic-beta: oauth-2025-04-20`
- On success: updates the `ClaudeUsage` entity, fires `notifier.notify()`
- On failure (network error, 401, 429, etc.): retains last known entity data, logs the error

### Lifecycle

```
startup
  └─ loop every 5 minutes:
       ├─ read ~/.claude/.credentials.json
       ├─ if missing or token expired → log warning, skip cycle
       └─ GET /api/oauth/usage → update ClaudeUsage entity → notify
```

Plugin emits no entity when credentials file is not found.

---

## Overview Component

**File:** `crates/overview/src/components/claude.rs`

Two `InfoCardWidget`s in a horizontal box, hidden when no `claude-usage` entity exists.

### Card layout

```
[claude-symbolic]  42%          [claude-symbolic]  85%
                   2h 15m                          6d 4h
```

- **Title:** percentage formatted as `"42%"`
- **Description:** remaining time until reset, e.g. `"2h 15m"` or `"6d 4h"` (no prefix label)
- **Icon:** `claude-symbolic` (bundled SVG asset, referenced by file path)
- Left card: 5-hour window
- Right card: 7-day window

The component hides entirely when no `claude-usage` entity is present.

### Time formatting

Relative time from now to reset timestamp:
- `< 1 hour`: `"45m"`
- `1h–24h`: `"2h 15m"`
- `1d+`: `"6d 4h"`

---

## Icon Asset

**File:** `plugins/claude/assets/claude-symbolic.svg`

Monochrome SVG using the Bootstrap Icons Claude path (viewBox `0 0 16 16`), suitable for GTK icon theming. Referenced by absolute file path resolved relative to the binary location at runtime.

---

## Files to Create/Modify

| Action | File |
|--------|------|
| Create | `crates/protocol/src/entity/ai.rs` |
| Modify | `crates/protocol/src/entity/mod.rs` (add `ai` module) |
| Create | `plugins/claude/Cargo.toml` |
| Create | `plugins/claude/bin/waft-claude-daemon.rs` |
| Create | `plugins/claude/src/lib.rs` (credential types) |
| Create | `plugins/claude/src/credentials.rs` (read credentials file) |
| Create | `plugins/claude/src/api.rs` (HTTP client) |
| Create | `plugins/claude/assets/claude-symbolic.svg` |
| Create | `plugins/claude/README.md` |
| Modify | `Cargo.toml` (add `plugins/claude` to workspace members) |
| Create | `crates/overview/src/components/claude.rs` |
| Modify | `crates/overview/src/components/mod.rs` (expose `claude` module) |
| Modify | `crates/overview/src/app.rs` (instantiate `ClaudeComponent`) |

---

## Dependencies

Additional crates needed in `plugins/claude/Cargo.toml`:

- `reqwest` with `features = ["json"]` — HTTP API calls
- `serde_json` — credential file parsing
- `tokio` with `features = ["full"]`
- `waft-plugin`, `waft-protocol`, `waft-i18n`
