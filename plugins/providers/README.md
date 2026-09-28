# AI Providers Plugin

Reports quota and usage windows for locally authenticated AI providers through the [`quotas`](https://crates.io/crates/quotas) Rust library. The daemon discovers credentials without requiring Waft-specific configuration and polls available providers every five minutes.

## Supported providers

The plugin uses the providers in `quotas::providers::ProviderKind::all()` plus the library's optional MiMo provider and currently supports:

- Claude
- Codex
- Cursor
- DeepSeek
- Google Antigravity/Gemini
- GitHub Copilot
- Grok
- Kimi
- MiMo (when a supported cookie or API key is available)
- MiniMax
- OpenRouter
- SiliconFlow
- Z.AI

Providers without credentials are skipped. Failed refreshes retain the last successful entity until a later refresh succeeds.

## Entity type

| Entity Type | URN Pattern | Description |
|---|---|---|
| `provider-usage` | `providers/provider-usage/{provider}` | Usage quota windows for one AI provider |
| `provider-config` | `providers/provider-config/{provider}` | Enablement and credential status for one AI provider |

Each usage entity contains the provider name, plan, fetch timestamps, an optional dashboard usage URL, and a list of windows. Windows carry provider-defined labels such as `5h`, `weekly`, or `monthly`, used/limit/remaining values, reset timestamps, optional window durations, and a flag identifying percentage-based capacity (for example Codex rate limits) versus credits/units. Configuration entities also expose the shared usage/remaining display mode.

## Configuration

Open **AI Providers** in `waft-settings` to enable or disable quota collection per provider and choose whether overview cards show consumed usage or leftover quota. Settings are stored in `~/.config/waft/providers.toml` and take effect without restarting the daemon. The default display mode is remaining quota.

Credentials are discovered using the same local files and environment variables supported by `quotas`, including Claude/Codex OAuth files, Cursor credentials, provider API keys, and OpenCode/Pi auth stores. The plugin only reads credentials and does not refresh or write them back.

## Dependencies

- [quotas](https://crates.io/crates/quotas) for provider discovery, authentication, and usage fetching
- Network access for provider usage endpoints
