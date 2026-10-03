# Plan: Reliable GNOME Online Accounts integration

## Status and implementation boundary

Technical design fixed after source-contract research and three consistency passes. The additive contracts were approved and implemented. Automated verification is recorded below; disposable-session Wayland/real-account release validation remains incomplete.

Implementation order: contract/fixture tests → GOA supervision → account actions → Settings UX → EDS handoff → release gates. Each phase follows red → green → refactor.

The user confirmed the exact API contracts below before behavioral coding (`Approved. Implement.`). Ownership and threading remain unchanged. Approval, automated verification, and supported-environment release validation are separate gates.

## Goal and limits

Waft converges to authoritative GOA account state after startup races, account changes, credential recovery, GOA owner replacement, and session-bus reconnection without a manual Waft restart. GOA-backed EDS calendars recover through the same classes of failure. Failure is visible, actionable, and never represented as a successful empty snapshot or completed remote synchronization.

In scope: accounts/providers/services, lifecycle supervision, bounded actions, system account-management handoff, Settings state/error handling, EDS source/view lifecycle, privacy-safe diagnostics, and regression tests.

Out of scope: storing credentials, implementing OAuth, using private GOA backend libraries, adding GTK to daemons, restarting/replacing dependency services, global protocol redesign, full iCalendar recurrence conformance, and unrelated agenda styling/rendering refactors. Existing recurrence expansion stays in place; source-scoped snapshot replacement prevents destructive partial-update handling without introducing a new recurrence engine.

External conditions are not promises: GOA may be uninstalled; providers may reject credentials; the network may remain down; GNOME Settings may fail to display; remote synchronization may take an unbounded time. Each condition has a defined failure/status path below. “No open design questions” does not mean those external failures become impossible.

## Verified contracts and source references

Research used upstream source, not search summaries as implementation authority. Pinned revisions:

- GOA: `c142281db4c444359c10b343244bf5eebf5e2535`.
- GNOME Control Center: `cc98ad952acb1937cdde9edffb298b4ac738fb17`.
- EDS: `c4f57c61c00505ce778e0bee175501675d591298`.

References:

1. [GOA D-Bus XML](https://github.com/GNOME/gnome-online-accounts/blob/c142281db4c444359c10b343244bf5eebf5e2535/data/dbus-interfaces.xml): `Account.ProviderType`, `AttentionNeeded`, `IsLocked`, all `*Disabled` properties, `Remove`, `EnsureCredentials`, and `Manager.IsSupportedProvider`.
2. [GOA provider registry](https://github.com/GNOME/gnome-online-accounts/blob/c142281db4c444359c10b343244bf5eebf5e2535/src/goabackend/goaprovider.c): no public D-Bus provider enumeration; current Microsoft 365 ID is `ms_graph`, not just the repository's `ms365` probe.
3. GOA provider feature implementations at the same revision: [Google](https://github.com/GNOME/gnome-online-accounts/blob/c142281db4c444359c10b343244bf5eebf5e2535/src/goabackend/goagoogleprovider.c), [Microsoft Graph](https://github.com/GNOME/gnome-online-accounts/blob/c142281db4c444359c10b343244bf5eebf5e2535/src/goabackend/goamsgraphprovider.c), [Nextcloud](https://github.com/GNOME/gnome-online-accounts/blob/c142281db4c444359c10b343244bf5eebf5e2535/src/goabackend/goaowncloudprovider.c), [WebDAV](https://github.com/GNOME/gnome-online-accounts/blob/c142281db4c444359c10b343244bf5eebf5e2535/src/goabackend/goawebdavprovider.c), [Exchange](https://github.com/GNOME/gnome-online-accounts/blob/c142281db4c444359c10b343244bf5eebf5e2535/src/goabackend/goaexchangeprovider.c), [IMAP/SMTP](https://github.com/GNOME/gnome-online-accounts/blob/c142281db4c444359c10b343244bf5eebf5e2535/src/goabackend/goaimapsmtpprovider.c), [Kerberos](https://github.com/GNOME/gnome-online-accounts/blob/c142281db4c444359c10b343244bf5eebf5e2535/src/goabackend/goakerberosprovider.c), [Fedora](https://github.com/GNOME/gnome-online-accounts/blob/c142281db4c444359c10b343244bf5eebf5e2535/src/goabackend/goafedoraprovider.c).
4. [Online Accounts panel](https://github.com/GNOME/gnome-control-center/blob/cc98ad952acb1937cdde9edffb298b4ac738fb17/panels/online-accounts/cc-online-accounts-panel.c): arguments `add PROVIDER_TYPE` and `ACCOUNT_ID` target creation and an existing account respectively.
5. [Control Center application](https://github.com/GNOME/gnome-control-center/blob/cc98ad952acb1937cdde9edffb298b4ac738fb17/shell/cc-application.c): CLI rejects a desktop environment without a `GNOME` token. Command dispatch is not a guarantee the requested account dialog displayed successfully.
6. EDS [Source XML](https://github.com/GNOME/evolution-data-server/blob/c4f57c61c00505ce778e0bee175501675d591298/src/private/org.gnome.evolution.dataserver.Source.xml), [Calendar XML](https://github.com/GNOME/evolution-data-server/blob/c4f57c61c00505ce778e0bee175501675d591298/src/private/org.gnome.evolution.dataserver.Calendar.xml), and [CalendarView XML](https://github.com/GNOME/evolution-data-server/blob/c4f57c61c00505ce778e0bee175501675d591298/src/private/org.gnome.evolution.dataserver.CalendarView.xml): `Source.UID/Data/ConnectionStatus`, `CredentialsRequired`, `Calendar.Online/Capabilities`, `Open`, `Close`, `Refresh`, `GetView`, and view `Start/Stop/Dispose/Complete`.
7. EDS [source registry](https://github.com/GNOME/evolution-data-server/blob/c4f57c61c00505ce778e0bee175501675d591298/src/libedataserver/e-source-registry.c), [collection](https://github.com/GNOME/evolution-data-server/blob/c4f57c61c00505ce778e0bee175501675d591298/src/libedataserver/e-source-collection.c), and [GOA bridge](https://github.com/GNOME/evolution-data-server/blob/c4f57c61c00505ce778e0bee175501675d591298/src/modules/gnome-online-accounts/module-gnome-online-accounts.c): effective enablement includes ancestors; GOA calendar-disabled binds to collection calendar-enabled.
8. EDS [calendar view client](https://github.com/GNOME/evolution-data-server/blob/c4f57c61c00505ce778e0bee175501675d591298/src/calendar/libecal/e-cal-client-view.c): removed identifiers encode `<uid>[\n<rid>]`, not necessarily a plain UID.
9. EDS [D-Bus calendar implementation](https://github.com/GNOME/evolution-data-server/blob/c4f57c61c00505ce778e0bee175501675d591298/src/calendar/libedata-cal/e-data-cal.c) and [meta backend](https://github.com/GNOME/evolution-data-server/blob/c4f57c61c00505ce778e0bee175501675d591298/src/calendar/libedata-cal/e-cal-meta-backend.c): the D-Bus reply waits for the backend's refresh handler, but the meta backend schedules subsequent remote work. A successful reply is not proof remote synchronization completed.

Compatibility is capability-based, not a claim that every distribution version was tested. Missing required interfaces/methods yield `unsupported-api`; optional differences use the explicit fallback rules below. Store minimal sanitized XML/key-file fixtures derived from these contracts in tests; no live account dumps.

## Corrections to the original diagnosis

- GOA's `*Disabled` properties are defined on the base Account interface even when the provider lacks that service. Property existence must not generate eight imaginary services.
- Absence of a service interface alone cannot prove lack of support: disabling a supported service asynchronously removes its interface.
- Add-account targeting exists upstream; Waft currently fails to pass the required arguments. The desktop guard is an additional concrete obstacle outside GNOME.
- EDS `GetBackendProperty("online")` is not in the pinned Calendar D-Bus interface. Read the boolean `Online` property instead. Offline is not synonymous with expired OAuth.
- `CalendarSync.last_refresh` is already documented in the protocol as a trigger timestamp. Preserve that meaning; fix the presentation, not its history by reinterpretation.
- `spawn_monitored` logs exits but does not supervise/restart. A new stream does not fix stale account snapshots or the original action connection.
- The central daemon's default action timeout is **5 seconds**. A synchronous multi-backend refresh cannot safely occupy that entire action path.

## Fixed architectural decisions

### Ownership and execution

- Waft only consumes GOA/EDS services. Never replace owners, restart services, change account files, delete accounts as repair, or call token/password APIs.
- Use one plugin-local supervisor for GOA and one for EDS. Both are Tokio-only. GTK rendering and callbacks remain main-thread-only; existing channel bridges stay intact.
- Supervisors own connections, unique-owner identities, generations, discovery jobs, retry deadlines, child processes, and publication. Action handlers use the supervisor, not a separate immutable connection.
- Reuse existing SDK infrastructure without changing its D-Bus monitor helpers globally. Add domain modules rather than a speculative universal supervisor.
- Locks protect short state copies/commits only. Never hold state locks across an await or notify callback.

### Deadlines and resource policy

Production defaults are fixed internal constants, not new user configuration:

| Operation | Limit/policy |
|---|---|
| Bus connection, activation, owner lookup, ordinary discovery call | 2 seconds each |
| GOA account mutation, including confirming reread | 4 seconds total from handler entry |
| GUI launch initial-exit observation | 250 milliseconds; no wait for an interactive window to close |
| EDS source open/view setup | 10 seconds total per source attempt |
| EDS initial view `Complete` | 10 seconds after `Start` |
| EDS background `Refresh` | 30 seconds per backend; never performed inline in the action handler |
| Failed dependency/setup recovery | 1, 2, 4, 8, 16, then 30 seconds, capped; no jitter |
| Invalidation burst | 50-millisecond quiet window, with a 250-millisecond maximum delay from first invalidation |
| Event/result queues | 256 entries; overflow marks dirty and triggers resnapshot rather than silently dropping correctness |
| EDS setup/refresh concurrency | At most 4 workers of each kind, one setup and one refresh per source |
| Cleanup | 1-second overall deadline for each retiring source's best-effort server calls; local cleanup always completes |
| Repeated identical error log | At most once per 30 seconds; always log state transitions |
| Pending UI action fallback | 6 seconds or disconnect, whichever comes first |

Reset backoff after successful recovery. Owner/source changes wake the corresponding retry immediately. Healthy GOA state is event-driven, never periodically polled. EDS retains its existing configurable background remote-refresh schedule; recovery does not depend on that schedule. Retry deadlines are failure recovery, not healthy-state polling.

If dependencies respond successfully within their operation budgets, GOA discovery converges within `10 + 2 × ceil(P / 4)` seconds of starting recovery, where P is the provider probe count (nine baseline IDs plus distinct custom account-provider IDs). This includes connection/activation/owner lookup, one account snapshot, provider probe batches, and coalescing slack; the baseline bound is 16 seconds. On an already-connected owner-appearance event, connection/activation waits are unnecessary. Without an owner-change event, add at most the current 30-second recovery delay. EDS source setup converges within 21 seconds after eligible source discovery when workers are available. Queue wait for more than four sources is explicit `pending`, not a failed deadline. These bounds exclude remote provider synchronization and continuous external mutation.

## Exact public contracts

All new fields on existing entities use serde defaults; all new status fields have defaults and unknown enum values decode to `Unknown` in new clients. Existing account/provider URNs stay unchanged. New entity types are additive and registered in manifests/static registry. No transport version bump or new message kind.

### GOA availability singleton

Entity type `online-accounts-status`, URN `gnome-online-accounts/online-accounts-status/singleton`:

| Field | Type / meaning |
|---|---|
| `accounts` | `Unknown / Starting / Ready / Recovering / Unavailable / Unsupported` |
| `providers` | Same availability enum, independently tracked |
| `last_accounts_snapshot` | Optional Unix seconds of the last successful authoritative account discovery |
| `last_providers_snapshot` | Optional Unix seconds of the last successful complete supported-provider probe set |
| `accounts_error` / `providers_error` | Optional `ProtocolError`, sanitized; no raw D-Bus messages with identities |
| `launch` | `Unknown / Idle / Accepted / Failed`; outcome of the latest account-management launch request |
| `launch_error` | Optional sanitized `ProtocolError` |
| `last_launch` | Optional Unix seconds of latest launch request |
| `launch_request_id` | Optional UUID identifying the latest launch; late results update status only while their UUID still matches |

Starting: first discovery in progress. Ready: successful authoritative discovery, including zero results. Recovering: a retryable failure after prior success. Unavailable: a retryable failure before first success. Unsupported: a required API is missing; retry on owner replacement. Source-specific validation failures additionally retry on changed source data. Unknown: older plugin or unrecognized serialized state.

During an outage, keep last-known account/provider entities in memory but mark the respective status degraded; disable mutations and provider-specific add requests until the relevant status is Ready. A successful empty snapshot removes stale entities. Do not persist account cache to disk. A restarted plugin starts Unknown/Starting, not with fabricated stale data.

Add `provider_type: String` (serde default empty) to `OnlineAccount`. Keep private capability provenance separate from `ServiceInfo`; no new meaning for its `enabled` field.

### Actions and results

Existing actions remain; add `open-account-settings` on `online-account` and `open-account-settings` on the availability singleton. Singleton launch opens the generic page and is allowed even when GOA itself is unavailable. Account-specific launch requires a currently known account ID; it does not require active credentials.

- `enable-service` / `disable-service`: success data `{ "outcome": "applied" }` only after reread confirms the requested `*Disabled` value on the current owner/account generation. Interface attachment may complete later; that is reconciled by signals. If the value reverted or cannot be confirmed within 4 seconds, return an error and resnapshot.
- `remove-account`: success data `{ "outcome": "removed" }` only after a successful authoritative snapshot confirms absence. A disappeared account before dispatch returns not-found, not successful removal.
- Provider `add-account`, account/singleton `open-account-settings`: `{ "outcome": "launch-accepted" }` means only accepted process dispatch. Creation/repair is proven by GOA state, never by launcher exit.
- Reject unknown entity/action/service combinations with errors; never fall through to null success.

Retain existing `error: String` compatibility. Add exported SDK wrapper `PluginActionError(ProtocolError)` implementing `Error`; handlers return it through `anyhow::Error`. Runtime downcasts to preserve details, otherwise keeps its existing `action.execution` fallback. Add `EntityStore.on_action_error_details(UUID, ProtocolError)` alongside existing string callbacks; synthesize `action.execution` for legacy messages. Settings uses the typed callback, not error-string parsing.

Fixed GOA codes: `goa.unavailable`, `goa.unsupported-api`, `goa.service-unsupported`, `goa.locked`, `goa.busy`, `goa.outcome-unconfirmed`, `goa.launch-unavailable`, `goa.launch-failed`, plus existing `entity.not-found`, `protocol.validation`, and `action.timeout`. Fixed EDS codes: `eds.unavailable`, `eds.unsupported-api`, `eds.source-invalid`, `eds.view-failed`, `eds.offline`, `eds.credentials-required`, `eds.certificate-required`, and `eds.refresh-failed`.

Unsupported API/service errors use Capability scope; invalid source/request uses Validation; unavailable connection uses Transport; absent account uses NotFound; deadlines use Timeout; other operation failures use Action. Only unavailable transport, view setup, offline, and background refresh errors are retryable. Unconfirmed mutations are not automatically retryable; discovering current state is safe, replaying a write/removal is not. Launch failures do not change account credential status.

### EDS status contracts

Keep `calendar-sync` URN and `last_refresh` semantics. `syncing` means Waft has an active backend refresh request batch, **not** completed provider synchronization.

Add defaulted fields:

- `availability`: the same availability enum, for EDS source discovery.
- `last_snapshot`: optional Unix seconds of successful source discovery.
- `refresh_outcome`: `Unknown / Never / Queued / Accepted / PartialFailure / Failed / Unsupported / NoBackends / Debounced / Cancelled`.
- `error`: optional sanitized `ProtocolError`.

Add `calendar-source-status`, URN `eds/calendar-source-status/{encoded-source-uid}`, fields:

- `source_uid`, `display_name`, optional `goa_account_id` inherited from `[GNOME Online Accounts] AccountId`.
- `state`: `Unknown / Pending / Opening / Loading / Watching / RetryWait / Disabled / Unavailable / Unsupported`.
- `online: Option<bool>` from `Calendar.Online`; missing/invalidated value means unknown until reread.
- `last_view_snapshot`, `last_refresh_attempt`, `last_refresh_accepted`, `last_event_delivery`: optional Unix seconds, each with its literal meaning.
- `error: Option<ProtocolError>`; no invented OAuth diagnosis.

Calendar event payload gains defaulted `source_uid: String`. New EDS occurrence identifier is `v2::{encode(source_uid)}::{encode(event.uid)}::{start_time}`. Encoding escapes every UTF-8 byte outside ASCII letters/digits/`-`/`_` as uppercase `%HH`, including `%`, `:`, and `/`. This is injective and safe in a single URN segment. Do not dual-publish legacy and v2 occurrences. No persisted event URNs exist in the current in-memory integration; generic older consumers treat IDs as opaque. Update agenda card/menu identity to use full entity URN so two calendars with the same UID/start cannot share expansion state.

`refresh` actions return within 1 second, including sources not ready yet:

- Ready eligible backends: enqueue one batch and return `{ "outcome": "queued" }`.
- Setup/discovery in progress: record one refresh intent for that source/discovery generation and return queued; execute when ready. Failed setup remains visible, never silently consumes intent.
- Ready with no eligible sources: `{ "outcome": "no-backends" }` and matching status.
- Unavailable/Unsupported source registry: action error, no success timestamp fabrication.
- Rate limited: `{ "outcome": "debounced" }`; a queued/in-flight batch's final outcome is not overwritten by this request result.

## Provider and service capability rules

### Provider discovery

Probe this fixed list with `Manager.IsSupportedProvider`, pinned to the current unique owner, four probes at a time: `google`, `ms_graph`, `ms365`, `owncloud`, `imap_smtp`, `exchange`, `kerberos`, `fedora`, `webdav`. Add unknown provider IDs found on authoritative accounts to the probe set, with their account-supplied names.

`false` means unsupported. A D-Bus error means failed discovery, not false. Commit a complete new provider list only when all probes finish successfully; otherwise retain the previous list with providers status degraded. Account readiness is independent. `UnknownMethod` marks provider discovery Unsupported, while accounts can remain Ready. Generic system-settings launch is still available. Probe on startup/owner recovery/account provider-set changes, never on a healthy periodic timer. New unconfigured provider IDs outside this list are not auto-enumerated: generic system settings is the explicitly documented route for them.

### Services

Pinned GOA source establishes these conservative baseline capabilities:

| Provider ID | Baseline services |
|---|---|
| `google` | mail, calendar, contacts |
| `ms_graph` | mail, calendar, contacts, files |
| `owncloud`, `webdav` | calendar, contacts, files |
| `exchange` | mail, calendar, contacts |
| `imap_smtp` | mail |
| `kerberos`, `fedora` | ticketing |
| `ms365` or other unverified legacy/custom ID | No assumed baseline; observed interfaces only |

Union baseline capabilities with known service interfaces actually observed on that account. Google's compile-time-optional files feature is included only after its Files interface has been observed. Keep observed capabilities through disablement and temporary owner changes for the same account/provider during the plugin lifetime; clear on account deletion/provider change. Do not retain them across a successful provider-discovery result that says the provider is no longer supported.

Only emit a service row when capability is established **and** its corresponding writable `*Disabled` property exists with a boolean value. Enabled state is `!Disabled`, not guessed from transient interface presence. Unknown custom/optional services that are already disabled at first discovery cannot be established through the public D-Bus API: omit the toggle and show “Manage additional services in account settings.” Never silently claim complete capability enumeration. This is a fixed conservative compatibility boundary, not a deferred discovery task.

Admin-disabled/configuration-specific services can reject or revert a write; confirming reread determines the outcome. Respect `IsLocked` by rejecting removal and disabling inline mutations in Waft, without claiming the upstream flag proves all service writes are technically prohibited.

`AttentionNeeded=true` maps to `NeedsAttention`; false maps to `Active`. Do not call `EnsureCredentials` in background or infer `CredentialsNeeded` from a generic attention flag. Keep the existing enum variant for wire compatibility; this integration does not emit it without an explicit credential-specific result. Repair takes place in system settings.

## GOA supervision and convergence algorithm

Extract `src/lifecycle.rs`, `src/actions.rs`, and `src/account_settings_launch.rs`; retain `src/dbus.rs`, `src/state.rs`, and `src/signal_monitor.rs` with explicit responsibilities.

1. Establish the connection and owned signal streams before activation/discovery. Watch `org.freedesktop.DBus.NameOwnerChanged` for `org.gnome.OnlineAccounts`; validate the bus daemon as sender for owner events.
2. Activate through normal D-Bus service activation; resolve its unique owner. Account calls and signal validation use that owner, not a well-known-name race. ObjectManager lives at `/org/gnome/OnlineAccounts`; properties are accepted only on current known account paths or that namespace, with the expected interfaces.
3. One continuously running reader increments an invalidation sequence for relevant Account properties (including invalidated entries), service-interface changes, additions/removals, and unknown account paths. It records owner loss immediately. Malformed relevant messages or reader overflow mark full resnapshot necessary.
4. Coalesce invalidations; issue a full `GetManagedObjects` account snapshot. GOA account sets are small; use whole-state reconciliation rather than multiple competing property-delta writers. Only one snapshot job is active.
5. Results carry connection generation, owner generation, account invalidation sequence, and provider-set revision. Reject obsolete results. If a result started before an already-observed relevant invalidation, discard it and schedule the next snapshot. Keep draining signals while discovery runs.
6. On a valid success, replace accounts/paths/capabilities atomically, publish once, and mark accounts Ready. Signals arriving after commit dirty state again and cause another snapshot. Convergence is required after a finite signal burst, not impossible atomicity across continuously changing remote state.
7. Probe providers separately when the provider set or owner changes; account events during provider probing do not continually restart account-independent probes. Commit only a complete current-generation result.
8. On owner/bus loss, cancel generation-owned jobs, retire subscriptions, mark status degraded, and schedule bounded recovery. Stream errors terminate that generation rather than looping on errors indefinitely. Both actions and monitoring acquire the next supervisor connection.
9. Generation-owned actions keep a cancellation token. Owner loss cancels local waiting and marks outcome unconfirmed if a request was sent. Late success cannot publish against a new owner.

Track a private account-incarnation counter in addition to owner generation: increment it on observed Account interface removal/re-addition and identity/path/provider replacement. Carry it through actions and reject obsolete results; do not let a reused path authorize an action against a replacement account. Use owner-pinned sender ordering and process relevant invalidations before dispatch/result commit.

Per-account mutations use a nonblocking permit: reject a second conflicting operation as `goa.busy`, rather than allowing it to queue beyond its 4-second budget. Revalidate readiness, account existence, and capability from an immediate authoritative snapshot before mutation; after the write, confirm through another snapshot within the same deadline. An action marks the account dirty before sending its mutation and retains that invalidation on timeout/cancellation. Remote side effects cannot be rolled back by cancelling a local future; never replay them automatically.

Limit a GOA coherent-snapshot burst to four attempts or 8 seconds, whichever comes first. If relevant invalidations prevent a valid commit within that budget, retain cached state, mark Recovering (Unavailable before first success), and resume at the recovery backoff deadline; owner changes still wake immediately. No unbounded tight resnapshot loop.

## Account-management launch algorithm

Use direct `tokio::process::Command`, not the current self-spawned `--add-account` helper and not a new GTK/native GOA helper:

- Generic: `gnome-control-center online-accounts`.
- Add: `gnome-control-center online-accounts add PROVIDER_TYPE`.
- Account/repair: `gnome-control-center online-accounts ACCOUNT_ID`.

Pass arguments directly, never through a shell. IDs come from authoritative state/provider discovery. Resolve the executable through PATH; missing executable is `goa.launch-unavailable`.

For Wayland sessions without a GNOME token in `XDG_CURRENT_DESKTOP`, set `XDG_CURRENT_DESKTOP=GNOME` **only in this child process**. Do not mutate Waft/global/session environment. This is an explicit compatibility shim for the upstream CLI guard, not a claim GNOME officially supports every compositor. Display/backend/panel failure is still a launch failure. Document the shim in the README; no new configuration option.

Supervisor owns the child immediately after spawn and reaps it in a monitored task. Suppress stdin/stdout/stderr to avoid forwarding account-sensitive diagnostics; store only executable name, sanitized error category, and exit status. Observe exit for 250 milliseconds:

- Spawn failure/nonzero exit in this window returns an action error and Failed launch status.
- Zero exit or a still-running child returns launch-accepted and Accepted status.
- A later nonzero exit updates launch status/error only if its launch UUID is still the singleton's latest request, displayed by Settings even after the action succeeded. Older children are still reaped/logged but cannot overwrite a newer launch result.

Generate the launch UUID before spawn and include `launch_request_id` in accepted action data.

Never describe accepted dispatch as “account created,” “credentials repaired,” or “dialog verified.” Block duplicate launches for the same target while its child lives. GOA `can_stop()` returns false while it owns active launcher children; owner/reader recovery still proceeds. Action cancellation after spawn does not kill an interactive system-settings process; child reaping survives action cancellation. Explicit plugin shutdown cancels internal monitoring jobs but does not kill the user's settings window; process adoption on process exit is an OS responsibility. Remove the obsolete helper entrypoint.

## Settings behavior

Update `pages/online_accounts.rs`, `online_accounts/{account_row,account_detail,add_account_dialog}.rs`, and app subscriptions:

- Subscribe to status/accounts/providers before initial idle reconciliation. Empty providers no longer imply “GOA not running.”
- Show distinct loading, recovering, unavailable, unsupported API, healthy/no accounts, and failed provider discovery states.
- Preserve last-known rows during outages with a stale indicator; disable inline mutations and provider-specific additions. Generic system settings remains available.
- Show generic attention status and “Open account settings” for repair. No token expiry countdown or unsupported credential diagnosis.
- Register action UUID/result handlers before dispatch. Handle `None` immediately, failure, 6-second fallback, and disconnect. Clear pending state exactly once; ignore later completion for a cleared UUID.
- Restore switches from the latest authoritative entity on failure; block notify handlers during programmatic updates. A successful write does not invent calendar readiness.
- Pop an account detail page only when the store confirms deletion, not immediately on clicking Remove. External deletion invalidates/pops its open detail page safely. Only unrelated pending actions remain active.
- Reconcile service-row insertion/removal and values with stable ordering. Do not recreate every account row on status changes.
- Show late launcher failure from the status singleton; launch UUIDs distinguish requests even when multiple launches share a timestamp. For legacy plugins without a status entity, show availability Unknown, allow existing legacy actions, retain local pending/error handling, and hide unsupported new actions.
- GTK tests exercise main-thread behavior; no Tokio D-Bus/process futures in GLib-local futures.

Localize all visible states/errors in en-US and cs-CZ. Map stable error codes to localized text; unknown/legacy errors use a generic message, not raw potentially sensitive server text.

## EDS supervision and source eligibility

Extract active daemon logic into domain modules under `plugins/eds/src/` with a library test seam. The unused legacy `plugins/eds/bin/dbus.rs` is not the implementation target.

SourceManager: `org.gnome.evolution.dataserver.Sources5`, `/org/gnome/evolution/dataserver/SourceManager`. Factory: `org.gnome.evolution.dataserver.Calendar8`, `/org/gnome/evolution/dataserver/CalendarFactory`. These names remain unchanged. Unknown service versions do not trigger guessed fallback names.

Use the GOA subscription/generation/resnapshot pattern for EDS source discovery. Watch source `Data`, ObjectManager additions/removals, SourceManager/factory owner changes, and every dynamic backend owner. Read all source key files, not only calendar sources:

- Calendar candidates have a parsed `[Calendar]` section, not merely a substring match.
- `[Data Source] Enabled` defaults true, `Parent` empty means root. Every resolved ancestor must be enabled. An ancestor `[Collection] CalendarEnabled` defaults true and must be true.
- Missing parents mark the child Pending until registry reconciliation resolves them. Cycles/invalid key-file values mark that source Unsupported with a validation diagnostic; do not open it.
- Changing an ancestor recomputes all descendants. Disabled/deleted sources immediately remove their events and retire their views; temporary registry/owner loss retains cached events with degraded source status.
- `[GNOME Online Accounts] AccountId` is diagnostic association only. No shared mutable state between plugins and no GOA injection of calendar entities.

Per-source lifecycle is `Pending → Opening → Loading → Watching`; transient failure enters RetryWait, disappearance enters Unavailable, disabling enters Disabled, absent required API enters Unsupported. Store generation-owned backend path/owner, active view, candidate view, tasks, retry deadline, and event snapshot. Source UID is never marked Watching before successful view completion. Every failure rolls back handles and permits a later attempt.

Setup sequence: factory `OpenCalendar` → resolve backend unique owner → Calendar `Open` → read Online/Capabilities → `GetView` → install sender/path/interface-filtered view streams → `Start` → collect initial ObjectsAdded until `Complete("", "")`. Error completion/timeout discards candidate state and schedules retry. Monitor Source `ConnectionStatus`/`CredentialsRequired` and Calendar Online/Error; sanitized diagnostics distinguish offline, credential-required, certificate, and generic backend error without storing certificates/tokens. Credential-required reasons are not all expired OAuth.

Ready/reconfigured/reenabled sources enqueue one refresh. Readiness/online/source changes wake retries and pending refresh intent immediately. EDS itself handles GOA authentication and subsequent refresh scheduling; Waft does not subscribe to credentials or bypass its bridge.

## Source-scoped calendar snapshots and cleanup

Use authoritative view replacement rather than interpreting partial modified/removed blobs as a complete recurring series:

1. During initial candidate population, parse/buffer ObjectsAdded into that candidate's source-local event map; do not publish partial batches as authoritative removal.
2. Any ObjectsAdded/Modified/Removed on a **steady active** view invalidates only its source snapshot. Coalesce the burst and create one replacement view on the existing backend, with the same query.
3. Keep the old published snapshot until replacement `Complete` succeeds. If a steady-view invalidation occurs after candidate setup began, discard/retry that candidate; do not publish a snapshot already known obsolete. Candidate live modifications/removals before Complete likewise dirty it. A discarded dirty candidate enters RetryWait using the same capped backoff as a setup failure, so continuous mutation cannot create an unbounded view-rebuild loop.
4. Atomically swap the completed source event map, then notify. The existing plugin runtime diffs entities, so unchanged events are not removed/readded to clients.
5. Stop/Dispose the retired view and remove its owned subscriptions. At most one active plus one candidate view per source. After backend-owner loss, only local cleanup is possible; do not call a new owner using an old path.
6. For teardown of a live backend, best-effort Stop/Dispose views and Close backend within the cleanup deadline; log failures and always release local tasks/subscriptions. Join aborted tasks. If multiple source handles share a backend owner/path, Close only when its reference count reaches zero.
7. Decode removed UID/RID payloads for validation/tests, but do not perform UID-wide cross-source deletion from them; replacement snapshots provide the canonical membership oracle.
8. Rebuild midnight query windows through the same candidate/swap mechanism. Keep the current actual window (today's local midnight through 60 days), correcting README drift. No separate destructive midnight reset.

Malformed iCalendar from one item records a sanitized parse diagnostic and excludes that item; do not log raw event descriptions/attendees. Existing recurrence-expansion limitations remain documented. Full agenda incremental widget rendering is a separate improvement; full URN identity is part of this plan to prevent cross-calendar coupling.

## Refresh scheduling and reporting

The action handler records/debounces/enqueues intent, then returns; the supervisor performs bounded background work. One global batch is active; new sources join a following batch. Calls for a source already queued/in-flight merge, not overlap. Queue state survives cancellation of the request's action future because no remote write is being rolled back.

Call Refresh only after backend Open succeeds. Read Capabilities; an explicit advertised list lacking `refresh-supported` yields Unsupported without repeated requests. Missing capabilities use one bounded Refresh attempt; failure becomes Failed, not a string-matched permanent unsupported marker. Delete the existing `format!(error).contains("Code11")` heuristic. Re-read capabilities on reopening/reconfiguration.

Aggregate outcome after all eligible sources finish:

- Every request acknowledged: Accepted.
- Some acknowledged, some failed: PartialFailure.
- No acknowledgments and at least one failure: Failed.
- All ineligible solely because refresh is unsupported: Unsupported.
- No eligible enabled sources: NoBackends.

Update each source's literal attempt/acceptance/event-delivery timestamps. Preserve `last_refresh` as the batch trigger timestamp even if later calls fail; UI labels it “Last refresh request,” not “Last synchronized.” Set syncing false on every terminal path, including shutdown/cancelled batch. Successful `Complete` indicates view snapshot delivery only; no `last_remote_sync` field is added.

Preserve `refresh_interval_secs`, `locked_refresh_interval_secs`, and `debounce_base_secs`. A zero active interval disables periodic requests instead of spinning; zero locked interval pauses them. Nonzero locked interval actually refreshes while locked. Replace hand-constructed logind session paths/Lock-only interpretation with `GetSession(XDG_SESSION_ID)` or `GetSessionByPID`, initial `LockedHint` read, and PropertiesChanged monitoring. Lock/unlock changes reschedule the deadline; unlock enqueues one immediate merged refresh and records it in debounce. Failure to monitor logind logs one diagnostic and falls back to the active interval. Config decode failure is logged before defaulting.

EDS `can_stop()` rejects stopping while setup/refresh jobs are active and preserves the existing 120-second post-refresh grace. That grace is resource policy, not evidence of completed synchronization. The Overview's ordinary visibility hide does not remove its subscriptions; no ownership/lifetime policy change is needed.

Overview subscribes to calendar-source-status/calendar-sync alongside events. Display source failure/disabled/offline information without calling a cached agenda current. Queue refresh intent on overlay open even if the singleton is not yet present; dispatch it once when discovered, then clear the client-side intent. Do not continually trigger refresh from status notifications.

## Implementation phases and file boundaries

### Phase 1 — Contract fixtures and red regression tests

- Add pinned XML/key-file fixtures, parser tests for base properties versus real capabilities, ms_graph probes, source ancestry/defaults, and view Complete/error/RID payloads.
- Put pure state/supervisor modules in the GOA library and extract an EDS library seam so tests exercise production logic, not a parallel test implementation.
- Build one fresh private `dbus-daemon` per lifecycle test, at a fixture-owned temporary Unix socket address. Inject a connection factory/address; never mutate process-global DBUS_SESSION_BUS_ADDRESS in concurrent tests.
- Fixture owns/kills/reaps its bus and mock-service processes. Restart the bus at the same address to exercise connection replacement. Missing `dbus-daemon` is an explicit failed prerequisite, not a skipped passing test.
- Fake services expose the referenced signatures and explicit barriers for snapshot, owner change, write confirmation, candidate completion, and background remote delivery. Give tests a 15-second outer deadline; inject short operation timers/backoff. Resource-churn tests get a 60-second deadline.

### Phase 2 — GOA availability and lifecycle

Implement status schema/registry/manifest and supervisor. Replace startup-only discovery, loose connection-wide handlers, and original action connection. Validate coherent snapshots and independent provider readiness before adding UI behavior.

### Phase 3 — Actions, typed errors, and launch

Implement exact action/results/deadlines, current-generation confirmation, busy permits, runtime cancellation hook, SDK typed-error wrapper, client detailed-error callback, child-only desktop shim, and launcher child lifecycle. Preserve generic SDK fallback behavior for every other plugin.

### Phase 4 — Settings integration

Implement status subscribers, action pending/result handling, authoritative rollback/removal, service-row reconciliation, and system-settings/attention UX. Add localized messages and legacy compatibility tests.

### Phase 5 — EDS and Overview handoff

Implement source registry supervision, ancestry eligibility, retryable setup, candidate snapshots, source-qualified IDs/status, background refresh results, scheduler corrections, safe cleanup, and Overview status/deferred refresh. Preserve old config defaults and document the zero-active-interval rule.

### Phase 6 — Release validation/documentation

Run all automated gates and disposable-session manual checks. Update both plugin READMEs, protocol metadata/action descriptions, locales, and test prerequisites. Record implementation status in this document without claiming unrun tests passed.

Touch map:

- GOA: `plugins/gnome-online-accounts/src/{dbus,signal_monitor,state,lib,lifecycle,actions,account_settings_launch}.rs`, daemon entrypoint, tests, locales, README.
- EDS: `plugins/eds/bin/waft-eds-daemon.rs`, new `src/{lib,lifecycle,source_registry,source_views,refresh_scheduler}.rs`, Cargo library declaration, tests, locales, README.
- Protocol: `crates/protocol/src/entity/{accounts,calendar,registry}.rs` and associated registry/serialization tests.
- SDK/client: `crates/plugin/src/{lib,runtime}.rs`, new `action_error.rs`, `crates/client/src/entity_store.rs`; no new generic D-Bus framework. Launcher also invokes the client disconnect hook in `crates/launcher/src/app.rs` to use per-type reconnect membership reconciliation.
- Settings: `src/app.rs`, `pages/online_accounts.rs`, `online_accounts/{account_row,account_detail,add_account_dialog}.rs`, locales.
- Overview: `src/app.rs`, `components/{calendar,agenda}.rs`, agenda card/menu identity, calendar status presentation, locales.

## Implementation verification record

Verified on the implementation worktree:

- `cargo build --workspace`: passed.
- `cargo test --workspace`: 1,406 passed, zero failed, 29 ignored across 81 reported suites (including doctests). Headless results do not establish graphical coverage for pre-existing tests that return when GTK initialization fails.
- `cargo clippy --workspace --all-targets -- -D warnings`: passed across the entire workspace (including GOA, EDS, protocol, SDK, client, Settings, Launcher, and Overview).
- Explicit ignored GTK contracts executed under isolated Broadway/private-bus sessions: both passed. Account tests exercise stable service rows, prop updates without write feedback, typed failures, local deadlines, disconnect/equal-status recovery, failed dispatch, administrator locks, and disappearance/navigation cleanup. Calendar tests exercise visible health, offline diagnostics, literal refresh-request labeling, and equal-status reconnect recovery. These are widget contracts, not Wayland or native account-dialog validation.
- Reconnect membership note addressed: the client retains cached rows through disconnect/partial snapshots, then consumes the existing per-type `StatusComplete` boundary to retire only unconfirmed missing URNs. Equal/live updates confirm present members; legacy/interrupted snapshots without completion stay conservative. Unit tests cover absent/retained members, empty snapshots, duplicate completion, type isolation, and interruption/legacy behavior. The Settings GTK contract now also checks missed account deletion across disconnect closes stale details after completed empty membership; Launcher invokes the same disconnect hook. No new protocol messages or public API signatures were needed.
- Reviewer-identified EDS retirement race: the new private-bus test `live_invalidation_during_old_view_disposal_is_not_swallowed` failed before the fix and passed afterward, including ten consecutive focused runs. It holds old-view Dispose, proves the new steady reader observed a modification during retirement, and requires convergence without another signal or midnight; view count remains bounded and backend cleanup completes. The fix retains the candidate snapshot revision through cleanup and explicitly checks it before waiting.
- Independent review round 3: **Code approval: OK; merge verdict: OK with notes**. The reviewer confirmed both the EDS retirement P1 and reconnect-membership limitation are resolved, with no further actionable P0/P1/P2 findings in the reviewed scope. Current transport ordering, completion authority, main-thread callbacks, Launcher integration, and Settings membership cleanup were reviewed against source and passing logs. The reviewer did not rerun commands. Remaining notes are manual validation, not code findings: publication as fully validated requires the disposable environment/results or an explicit manual-gate waiver documenting unrun checks. Report: `/tmp/waft-reliability-independent-review.md` (temporary review artifact).
- Stress coverage: 100 GOA owner cycles without connection growth; 100 native fixture children reaped with launch-target slots cleared; 100 EDS view replacements with Stop/Dispose/shared-backend cleanup. Private-bus suites cover late dependencies, owner/bus restart, failed snapshots/candidates, action fencing/cancellation, independent provider recovery, and consumed pinned wire fixtures.

The host lacks an installed `gtk4-layer-shell`; build/test/lint used the verified official Arch package extracted (not installed) to `/tmp/waft-native-deps`. `PKG_CONFIG_PATH=/tmp/waft-native-deps/usr/lib/pkgconfig` supplies native build discovery; test execution additionally uses `LD_LIBRARY_PATH=/tmp/waft-native-deps/usr/lib`. The extracted pkg-config prefix points into that temporary directory; repository build configuration is unchanged.

To explicitly execute the GTK contracts with a supported display, use:

```bash
cargo test -p waft-settings -p waft-overview gtk_ -- --ignored --test-threads=1
```

Broadway execution used a temporary runtime/config/data directory, memory GSettings, `GTK_A11Y=none`, `GDK_BACKEND=broadway`, and `BROADWAY_DISPLAY=:73`. Its `dbus-run-session --config-file=...` configuration omits activation directories, preventing tests from launching real GOA/EDS/portal services. The bus allows temporary peers to own names/send/receive; no user-session bus or process-global environment is modified.

Publication authorization: the user explicitly approved committing and pushing to `origin/master` with the manual checks documented as unrun. This waives the manual gate for this code publication only; it does not certify native dialogs, authentication repair, or real-provider synchronization.

Remaining release gate: no Wayland compositor/disposable real account is available on this host. The manual dependency/authentication/offline/suspend/remote-event checks below are unrun, and no dialog opening, credential repair, or remote freshness is claimed. Automated stress checks establish the stated peer/view/child-slot oracles, not a measurement of every internal Tokio task allocation.

## Required automated acceptance matrix

| Test | Exact oracle |
|---|---|
| Healthy, empty GOA | Accounts Ready with zero rows; providers independently Ready/Unsupported; no false not-running banner |
| Base Account has all Disabled properties, mail-only provider | Only supported mail row; no imaginary calendar toggle |
| Supported calendar disabled at first snapshot | Conservative known-provider calendar row remains available for enabling; unknown-provider fallback directs to system settings |
| ms_graph supported, ms365 false | Microsoft Graph provider emitted under its actual ID; add launch uses that same ID |
| One provider probe fails | No partial new provider list; providers degraded; accounts remain usable |
| Mutation during discovery | Final entities equal successful fake-service snapshot after signal burst settles |
| GOA absent, appears later | Automatic current-owner discovery within stated healthy-response budgets |
| Owner replacement without bus loss | Accounts/paths/providers refreshed; both next action and reader use new owner |
| Bus restart | Old jobs/streams retired; new connection discovers and handles actions; no stale response commit |
| Foreign signal matching interface/path | No state change or invalidation attributed to the managed service |
| Invalidated property/service-only change | Snapshot rescheduled; final rows/status match service state |
| Failed snapshot after prior success | Cached rows retained and stale; no deletion from failed/partial state |
| Write, concurrent write, timeout/cancellation | Applied only on confirmed value; busy conflict rejected; pending clears once; no automatic write replay |
| Removed account or account path reused | Not-found/authoritative removal; old generation cannot mutate a replacement account |
| Launcher arguments/environment | Exact provider/account args; GNOME shim child-only; no shell interpolation or parent-env mutation |
| Launcher missing/immediate/late nonzero exit | Typed error for immediate failure, late status failure after accepted dispatch; child reaped |
| GUI child alive after action cancellation | Child not killed, still supervised/reaped; no duplicate launch |
| Source/ancestor disabled, missing, cycle, reenabled | Correct Disabled/Pending/Unsupported/Watching states; disabled/deleted events removed, recoverable sources reopened |
| Setup/GetView/Start/Complete failure then success | Failed handles rolled back; automatic retry produces one Watching source without midnight/restart |
| Registry/factory/backend restart | Old generation invalidated; view resnapshot restores current source events |
| UID/start shared by two calendars | Two source-qualified entities/cards; independent menu state; removing one source retains the other |
| Single detached occurrence modification/removal | Candidate snapshot replaces only its source; surviving sibling occurrences remain according to fake-service authoritative snapshot |
| Change during replacement view initialization | Dirty candidate not committed; retry converges; at most two views per source |
| Refresh before backend ready | One retained intent runs after setup; no silent skipped-success result |
| Refresh zero backends/unsupported/all errors/mixed success | Exact result/status category; no fictional remote completion timestamp |
| Backend acknowledges then delivers events later | Accepted first; event-delivery timestamp advances only on delivery |
| Repeated opens/refreshes/cancellation | Single-flight requests, explicit debounce, syncing eventually false |
| Zero/nonzero locked intervals and initial lock | Pause or configured locked cadence; initial state correct; unlock produces one merged request |
| Legacy payloads/plugins and unknown enum | Default/Unknown fallback; old string-error subscribers unchanged; new fields do not break old decoders |
| 100 owner/view/launcher churn cycles | Task/subscription/view/child counters return to baseline; all queues remain bounded |

SDK/client tests additionally prove typed error details survive the plugin → daemon → app path and that fallback legacy callbacks receive exactly one error. GTK tests require a supported display environment and prove pending/error/row lifecycle on the main thread. Full-source snapshot computation must produce no EntityUpdated/Removed messages for unchanged events.

## Commands and manual release gates

```bash
cargo test -p waft-plugin-gnome-online-accounts -p waft-plugin-eds
cargo test -p waft-protocol -p waft-plugin -p waft-client
cargo test -p waft-settings -p waft-overview
cargo build --workspace
cargo test --workspace
cargo clippy -p waft-plugin-gnome-online-accounts -p waft-plugin-eds \
  -p waft-protocol -p waft-plugin -p waft-client \
  -p waft-settings -p waft-overview --all-targets -- -D warnings
cargo fmt --all -- --check
```

Private-bus tests launch their own bus; no wrapper/global session override is required. Do not run restart/credential failure tests against the user's actual session/account.

In a disposable Wayland session/account, record actual GOA/EDS/Control Center versions and results for: late dependencies; external add/remove; calendar off/on; independent GOA/EDS restarts; account needing attention repaired through system UI; missing launcher and non-GNOME shim; offline/online; suspend/resume; remote event create/change/delete. Measure refresh acknowledgment separately from actual event delivery. A permanently unavailable provider or launcher must produce the documented degraded/error state, not be counted as a recovery failure or silently counted as success.

Privacy gate: no raw accounts.conf, emails, credentials, tokens, certificates, event bodies, or attendee lists in added fixtures/logs. Existing EDS event-content logging in the touched signal/parse paths must be removed or sanitized as part of the change. Logs identify operation, generation, counters, and stable error category only.

Completion requires every automated oracle, bounded-resource check, compatibility check, and supported-environment manual gate to pass with recorded evidence. Unrun tests/environment blockers remain incomplete. A normal-login demo alone is not acceptance.

## Plan consistency passes

1. **Contract pass:** resolved service support versus Disabled properties; pinned provider IDs and fallback behavior; fixed GNOME Settings targeting/desktop guard; distinguished Calendar.Online from nonexistent property methods; fixed view completion/removal/refresh meanings.
2. **Execution pass:** resolved 5-second action-budget conflict through background EDS refresh; defined snapshot generation/dirty-state convergence; fixed per-account mutation ordering, cancellation, child lifetime, resource budgets, source ancestry, source-qualified identity, and legacy error/status compatibility.
3. **Final consistency pass:** corrected latency arithmetic for nine probes at concurrency four, defined account-incarnation guards for reused paths, specified EDS error codes/scopes, and prevented older child exits from overwriting newer launch outcomes.

There are no remaining technical choices delegated to implementation. Runtime variation is handled by specified capability/error/fallback rules, not by “investigate later” tasks. The required pre-coding API confirmation was obtained; supported-environment release validation remains a separate gate.
