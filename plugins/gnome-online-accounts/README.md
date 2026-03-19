# GNOME Online Accounts Plugin

Monitors GNOME Online Accounts (GOA) via D-Bus and exposes account and provider entities.

## Entity Types

### `online-account`

One entity per configured GOA account.

| Field | Type | Description |
|-------|------|-------------|
| `id` | `String` | GOA account ID (e.g. `account_1234567890`) |
| `provider_name` | `String` | Provider display name (e.g. "Google", "Nextcloud") |
| `presentation_identity` | `String` | User-facing identity (e.g. "user@gmail.com") |
| `status` | `AccountStatus` | `Active`, `CredentialsNeeded`, or `NeedsAttention` |
| `services` | `Vec<ServiceInfo>` | Per-service enabled/disabled state (only services the provider supports) |
| `locked` | `bool` | Whether the account is administrator-locked (removal blocked) |

**URN:** `gnome-online-accounts/online-account/{account-id}`

#### Actions

| Action | Params | Description |
|--------|--------|-------------|
| `enable-service` | `{ "service_name": "calendar" }` | Enable a service on this account |
| `disable-service` | `{ "service_name": "calendar" }` | Disable a service on this account |
| `remove-account` | | Remove the account from GOA. Fails if account is locked. |

### `online-account-provider`

One entity per supported GOA provider type. Used by the settings UI to show available providers for adding new accounts.

| Field | Type | Description |
|-------|------|-------------|
| `provider_type` | `String` | Provider identifier (e.g. "google", "ms365", "owncloud") |
| `provider_name` | `String` | Human-readable display name (e.g. "Google", "Microsoft 365") |
| `icon_name` | `Option<String>` | Themed icon name (e.g. "goa-account-google") |

**URN:** `gnome-online-accounts/online-account-provider/{provider-type}`

#### Actions

| Action | Description |
|--------|-------------|
| `add-account` | Launch the add-account flow for this provider. Spawns a helper subprocess that opens GNOME Settings to the online accounts page. The new account is detected automatically via D-Bus `InterfacesAdded` signal. |

#### Known Provider Types

The plugin checks these provider types via `Manager.IsSupportedProvider` D-Bus call. Only supported providers are emitted as entities:

| Type | Display Name |
|------|-------------|
| `google` | Google |
| `ms365` | Microsoft 365 |
| `owncloud` | Nextcloud |
| `imap_smtp` | IMAP and SMTP |
| `exchange` | Microsoft Exchange |
| `kerberos` | Enterprise Login (Kerberos) |
| `fedora` | Fedora |
| `webdav` | WebDAV |

## D-Bus Interfaces

| Bus | Service | Path | Interface | Usage |
|-----|---------|------|-----------|-------|
| Session | `org.gnome.OnlineAccounts` | `/org/gnome/OnlineAccounts` | `org.freedesktop.DBus.ObjectManager` | Enumerate accounts and services |
| Session | `org.gnome.OnlineAccounts` | `/org/gnome/OnlineAccounts/Accounts/*` | `org.gnome.OnlineAccounts.Account` | Account properties, Remove |
| Session | `org.gnome.OnlineAccounts` | `/org/gnome/OnlineAccounts/Accounts/*` | `org.gnome.OnlineAccounts.*` | Service-specific interfaces (Mail, Calendar, etc.) |
| Session | `org.gnome.OnlineAccounts` | `/org/gnome/OnlineAccounts/Manager` | `org.gnome.OnlineAccounts.Manager` | `IsSupportedProvider` for provider discovery |

## How It Works

1. **Account discovery**: On startup, enumerates GOA objects via `ObjectManager.GetManagedObjects`, extracting account properties and per-service enabled state
2. **Provider discovery**: Checks each known provider type via `Manager.IsSupportedProvider` to determine which are available on the system
3. **Signal monitoring**: Watches `InterfacesAdded`, `InterfacesRemoved`, and `PropertiesChanged` D-Bus signals for real-time updates when accounts are added, removed, or modified
4. **Add-account flow**: Spawns itself with `--add-account <provider-type>` flag, which opens `gnome-control-center online-accounts` to trigger the native GOA dialog (handles OAuth, WebKit, form-based flows)

## Dependencies

- **goa-daemon** (GNOME Online Accounts daemon) running on session D-Bus
- **gnome-control-center** (GNOME Settings) for the add-account flow (optional; add-account action fails gracefully without it)

## Configuration

```toml
[[plugins]]
id = "gnome-online-accounts"
```

No plugin-specific configuration options.
