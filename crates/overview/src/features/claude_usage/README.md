# Claude Usage Plugin

Real-time Claude API usage tracking for waft-overview. Displays message counts and token usage from the Anthropic Admin API in the header bar.

## Features

- 📊 **Real-time metrics**: Message count and token usage
- ⏱️ **Configurable windows**: Session, Hourly, Daily, or Weekly tracking
- 🔄 **Periodic updates**: Automatic refresh at configurable intervals
- 💪 **Resilient**: Graceful error handling with last-good data preservation
- 🎨 **Clean UI**: Minimal, readable display with smart number formatting

## Installation

### Prerequisites

1. **Admin API Key**: Get one from [Anthropic Console → API Keys](https://console.anthropic.com/settings/keys)
   - Must start with `sk-ant-admin` (e.g., `sk-ant-admin01-...`)
   - Regular API keys will not work

### Configuration

Add to your `~/.config/waft/config.toml`:

```toml
[[plugins]]
id = "plugin::claude-usage"
api_key = "sk-ant-admin-01234567..."  # Your Admin API key (REQUIRED)
```

That's it! The plugin will use sensible defaults for everything else.

## Configuration Options

### Basic Options

```toml
[[plugins]]
id = "plugin::claude-usage"
api_key = "sk-ant-admin-..."    # REQUIRED: Your Admin API key
update_interval = 300            # Optional: Seconds between updates (default: 300)
window = "session"               # Optional: Time window to track (default: "session")
```

### Time Windows

| Window | Description | Use Case |
|--------|-------------|----------|
| `session` | Since app started | Track current session usage |
| `hourly` | Last 60 minutes | Recent activity monitoring |
| `daily` | Last 24 hours | Daily usage tracking |
| `weekly` | Last 7 days | Weekly usage overview |

**Example - Track last 24 hours:**

```toml
[[plugins]]
id = "plugin::claude-usage"
api_key = "sk-ant-admin-..."
window = "daily"
update_interval = 600  # Update every 10 minutes
```

### Metrics Display

Control which metrics appear in the widget:

```toml
[[plugins]]
id = "plugin::claude-usage"
api_key = "sk-ant-admin-..."

[plugins.metrics]
show_message_count = true   # Show "Messages: X" (default: true)
show_token_usage = true     # Show "Tokens: X.XK" (default: true)
show_rate_info = false      # Future: rate limit info (default: false)
```

**Example - Minimal display (tokens only):**

```toml
[[plugins]]
id = "plugin::claude-usage"
api_key = "sk-ant-admin-..."

[plugins.metrics]
show_message_count = false
show_token_usage = true
```

## Display Format

The widget shows in the header bar:

```
[Icon] Messages: 1,234
       Tokens: 125.3K
```

### Number Formatting

- **Tokens**: Human-readable with K/M suffixes
  - `1,234` → `1.2K`
  - `125,340` → `125.3K`
  - `1,234,567` → `1.2M`

- **Messages**: Comma-separated for readability
  - `42` → `42`
  - `1234` → `1,234`
  - `1234567` → `1,234,567`

### Widget States

1. **Loading** (⟳): Spinner while fetching initial data
2. **Loaded** (✓): Icon + metrics display
3. **Error** (✗): Error message shown

On refresh errors, the last successfully fetched data remains visible.

## Architecture

### File Structure

```
claude_usage/
├── README.md       # This file
├── mod.rs          # Plugin implementation and configuration
├── api.rs          # Anthropic Admin API client
└── values.rs       # Data types and formatters
```

### How It Works

1. **Plugin Registration** (`app.rs`):
   - Loaded at application startup
   - Configured from TOML settings
   - Registered with weight 15 (between clock and weather)

2. **Initial Fetch**:
   - On `create_elements()`, widget shows loading state
   - Spawns async task to fetch usage data from API
   - Updates widget to loaded/error state

3. **Periodic Updates**:
   - Uses `glib::timeout_add_local()` for periodic polling
   - Runs every `update_interval` seconds (default: 300)
   - Updates on success, preserves last-good data on failure

4. **API Integration**:
   - Uses `spawn_on_tokio()` bridge for async HTTP
   - Prevents glib busy-polling with reqwest
   - Aggregates usage buckets from Anthropic API

### API Details

**Endpoint:**
```
GET https://api.anthropic.com/v1/organizations/usage_report/messages
```

**Headers:**
```
x-api-key: sk-ant-admin-...
anthropic-version: 2023-06-01
```

**Query Parameters:**
- `starting_at`: ISO 8601 timestamp (calculated from window)
- `ending_at`: ISO 8601 timestamp (now)
- `bucket_width`: `"1m"`, `"1h"`, or `"1d"` (based on window)

**Response Aggregation:**
The plugin sums all buckets to calculate:
- Total messages (`count`)
- Input tokens (`input_tokens`)
- Output tokens (`output_tokens`)
- Cache read tokens (`cache_read_tokens`)
- Total tokens (sum of above)

## Troubleshooting

### Authentication Errors

**Error:** `Authentication failed - check API key`

**Solutions:**
1. Verify API key starts with `sk-ant-admin` (e.g., `sk-ant-admin01-...`)
2. Check key is copied correctly (no extra spaces)
3. Ensure it's an Admin API key, not a regular API key
4. Verify key hasn't been revoked in Anthropic Console

### Network Errors

**Error:** `Failed to fetch usage data`

**Solutions:**
1. Check internet connection
2. Verify access to `api.anthropic.com`
3. Check firewall settings
4. View logs: `journalctl -f | grep claude-usage`

### Rate Limiting

**Error:** `Rate limited - try again later`

**Solutions:**
1. Increase `update_interval` (recommended: ≥ 60 seconds)
2. Wait a few minutes before restarting
3. Check Anthropic Console for rate limit details

### Widget Not Showing

**Solutions:**
1. Verify plugin is enabled in `config.toml`
2. Check configuration has valid TOML syntax
3. Look for errors in logs: `RUST_LOG=debug ./waft-overview`
4. Rebuild: `cargo clean && cargo build --workspace`

### Zero Usage Displayed

This may be correct if:
- You haven't made any API calls yet
- Tracking window is before your first API call
- Session window was just started

Try switching to `window = "daily"` or `window = "weekly"` to see historical usage.

## Development

### Building

```bash
cd /path/to/sacrebleui
cargo build --workspace
```

### Testing

1. Add test configuration to `~/.config/waft/config.toml`
2. Run: `./target/debug/waft-overview`
3. Toggle overlay: `./target/debug/waft-overview toggle`
4. Check logs: `RUST_LOG=debug ./target/debug/waft-overview`

### Debug Logging

Enable detailed logging:

```bash
RUST_LOG=debug ./target/debug/waft-overview 2>&1 | grep claude-usage
```

Expected output:
```
[DEBUG] [claude-usage] Fetching initial usage for window: Session
[DEBUG] [claude-usage] Loaded: 42 messages, 125340 tokens
[DEBUG] [claude-usage] Fetching usage update
```

## Security Considerations

- **API Key Storage**: Stored in plain text in `config.toml`
  - Secure file permissions: `chmod 600 ~/.config/waft/config.toml`
  - Don't commit `config.toml` to version control

- **Read-Only Access**: Plugin only reads usage data
  - Cannot make API calls to Claude
  - Cannot modify account settings
  - Cannot access other organization data

- **Network Traffic**: Only communicates with `api.anthropic.com`
  - No telemetry or analytics
  - No data sent to third parties

## Performance

- **Memory**: Minimal (~100KB per widget instance)
- **CPU**: Negligible (only during API fetches)
- **Network**: Low bandwidth
  - Default: 1 request per 5 minutes
  - Typical response: ~1-5KB JSON

### Recommended Settings

- **Development**: `update_interval = 60` (1 minute)
- **Production**: `update_interval = 300` (5 minutes, default)
- **Low bandwidth**: `update_interval = 600` (10 minutes)

## Comparison with Weather Plugin

This plugin follows the same architecture as the weather plugin:

| Aspect | Weather | Claude Usage |
|--------|---------|--------------|
| API | Open-Meteo | Anthropic Admin |
| Update Interval | 10 minutes | 5 minutes |
| Weight | 20 | 15 |
| State Management | Loading/Loaded/Error | Loading/Loaded/Error |
| Async Bridge | spawn_on_tokio | spawn_on_tokio |
| Error Strategy | Keep last-good | Keep last-good |

## Future Enhancements

Potential features for future releases:

- [ ] Cost tracking (USD amounts)
- [ ] Per-model usage breakdown
- [ ] Click to open Anthropic Console
- [ ] Visual alerts for approaching limits
- [ ] Historical usage graphs
- [ ] CSV/JSON export
- [ ] Multiple organization support
- [ ] Custom time ranges

## License

Same as waft-overview parent project.

## Contributing

When modifying this plugin:

1. Follow the weather plugin pattern for consistency
2. Use `spawn_on_tokio()` for all async HTTP calls
3. Preserve last-good data on refresh failures
4. Add debug logging for state changes
5. Update this README with new features
6. Test with various window configurations

## Support

For issues or questions:

1. Check this README first
2. Review logs with `RUST_LOG=debug`
3. Check Anthropic API status
4. Open issue on waft repository with logs

---

**Plugin ID**: `plugin::claude-usage`
**Widget Slot**: `Slot::Header`
**Widget Weight**: `15`
**API Version**: `2023-06-01`
**Minimum Update Interval**: `60` seconds (recommended)
