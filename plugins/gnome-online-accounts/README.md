# GNOME Online Accounts Plugin

Tokio-only GOA integration with authoritative account reconciliation, independent provider discovery, and observable native account-management dispatch. Waft does not implement OAuth, retrieve credentials, replace GOA ownership, or restart services. GOA authenticates accounts; EDS independently discovers their enabled calendar sources.

## Entities

| Entity | URN | Purpose |
|---|---|---|
| `online-account` | `gnome-online-accounts/online-account/{account-id}` | Provider type/name, presentation identity, status, established services, admin lock |
| `online-account-provider` | `gnome-online-accounts/online-account-provider/{provider-type}` | Supported provider name/type and optional themed icon |
| `online-accounts-status` | `gnome-online-accounts/online-accounts-status/singleton` | Independent account/provider availability and latest settings-launch outcome |

Account/provider URNs are unchanged. `provider_type` is defaulted for older payloads. Status availability is Starting, Ready, Recovering, Unavailable, Unsupported, or Unknown. Ready with zero entities is successful empty discovery, not a missing service. Status includes separate successful snapshot timestamps/errors and launch state, timestamp, error, and request UUID.

`AttentionNeeded` maps to NeedsAttention, not automatically CredentialsNeeded. The legacy CredentialsNeeded variant remains decodable, but this plugin does not infer credential expiry or call EnsureCredentials in the background.

## Service Capabilities

Base `*Disabled` properties do not prove a provider supports those services. Toggles require an established provider baseline or observed service interface **and** a corresponding boolean disabled property. Previously observed capabilities survive disablement and temporary owner loss, but not account deletion/provider change or a successful unsupported-provider result.

Baselines: Google/Exchange mail/calendar/contacts; Microsoft Graph (`ms_graph`) additionally files; Nextcloud/WebDAV calendar/contacts/files; IMAP/SMTP mail; Kerberos/Fedora ticketing. Google's optional files feature and unverified `ms365`/custom features require an observed interface. Unknown disabled services at first discovery belong in native account settings, not invented Waft toggles.

## Actions

| Target | Action | Result |
|---|---|---|
| account | `enable-service` / `disable-service`, `{ "service_name": "calendar" }` | `applied` only after authoritative confirmation |
| account | `remove-account` | `removed` only after confirmed absence; admin lock prevents mutation/removal |
| account | `open-account-settings` | Native settings targeted at the known account ID |
| provider | `add-account` | Native settings targeted at that provider |
| status singleton | `open-account-settings` | Generic native settings, including when GOA is unavailable |

Account mutations have a four-second total budget and a nonblocking per-account conflict permit. Owner/account-incarnation changes, cancellation, and unconfirmed results schedule reconciliation, never automatic destructive replay. Typed errors preserve stable codes while retaining legacy string compatibility.

### Native Settings Dispatch

Direct command arguments, without a shell or self-spawned helper:

- Generic: `gnome-control-center online-accounts`
- Add: `gnome-control-center online-accounts add PROVIDER_TYPE`
- Account/repair: `gnome-control-center online-accounts ACCOUNT_ID`

On non-GNOME Wayland sessions, the child receives `XDG_CURRENT_DESKTOP=GNOME` to satisfy GNOME Settings' desktop guard. No Waft/global/session environment is changed. This is a compatibility shim, not a promise that every compositor/backend supports the panel.

After a 250-ms initial-exit observation, `launch-accepted` means accepted process dispatch, **not** account creation, a verified dialog, or credential repair. Missing/nonzero launch is an error. Children are reaped even after action cancellation; a later failure updates status only for the latest launch UUID. Duplicate targets are blocked while their child lives. Stopping the plugin does not kill the user's settings window.

## Discovery and Recovery

Session service `org.gnome.OnlineAccounts` exposes ObjectManager at `/org/gnome/OnlineAccounts`, Manager at `/org/gnome/OnlineAccounts/Manager`, and Account interfaces below `/org/gnome/OnlineAccounts/Accounts`.

Subscribe before discovery; validate unique owners; treat properties/interface signals as invalidations. Failed discovery retains stale entities with degraded status. Coherent snapshot bursts are bounded to four attempts/eight seconds; recovery backoff is 1, 2, 4, 8, 16, then 30 seconds. Healthy accounts/providers are event-driven, not periodically polled.

GOA has no public provider enumeration. Probe `google`, `ms_graph`, `ms365`, `owncloud`, `imap_smtp`, `exchange`, `kerberos`, `fedora`, `webdav`, plus IDs observed on accounts, four at a time. False means unsupported; a failed call degrades provider discovery without hiding accounts. Other unconfigured providers remain accessible through generic native settings.

## Dependencies and Configuration

GOA must be installed and D-Bus activatable. GNOME Settings is optional; launch actions fail explicitly when it is absent or cannot use a graphical session.

```toml
[[plugins]]
id = "gnome-online-accounts"
```

No plugin-specific options. Native-launch failures never change account credential status.

## Tests

`cargo test -p waft-plugin-gnome-online-accounts` includes conservative-capability fixtures and private-bus lifecycle tests. `dbus-daemon` is required; tests never contact the user session or alter its D-Bus environment. Wire fixtures reference pinned upstream GOA contracts.
