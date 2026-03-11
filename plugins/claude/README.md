# Claude Usage Plugin

Displays current Claude Code API usage by reading locally cached credentials and polling the Anthropic usage API. Reports utilization percentages for the 5-hour and 7-day rate-limit windows.

## Entity Types

| Entity Type | URN Pattern | Description |
|---|---|---|
| `claude-usage` | `claude/claude-usage/me` | Current Claude API usage for the authenticated account |

### `claude-usage` entity

- `five_hour_utilization: f64` — Utilization percentage for the 5-hour rate-limit window, 0.0–100.0
- `five_hour_reset_at: i64` — Unix timestamp in milliseconds when the 5-hour window resets
- `seven_day_utilization: f64` — Utilization percentage for the 7-day rate-limit window, 0.0–100.0
- `seven_day_reset_at: i64` — Unix timestamp in milliseconds when the 7-day window resets

Returns no entity if credentials are missing or the stored access token has expired.

## Actions

None. This is a display-only plugin.

## Configuration

No configuration required. The plugin is zero-config and works automatically once Claude Code is installed and authenticated.

## How It Works

1. Reads `~/.claude/.credentials.json` to obtain the OAuth access token.
2. Polls `GET https://api.anthropic.com/api/oauth/usage` every 5 minutes using the token from step 1.
3. Emits a `claude-usage` entity with the utilization values returned by the API.
4. If the credentials file is absent or the access token is expired, no entity is emitted and the plugin waits until the next poll cycle.

## Dependencies

- [Claude Code](https://claude.ai/code) must be installed and the user must be authenticated (`claude login`). The plugin reads credentials written by Claude Code to `~/.claude/.credentials.json`.
- Network access (HTTP via reqwest to `api.anthropic.com`).
