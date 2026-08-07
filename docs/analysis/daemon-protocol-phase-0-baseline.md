# Daemon protocol Phase 0 baseline

## Status

Phase 0 baseline completed for protocol hardening planning.

## Scope

This document freezes the current daemon protocol surface before versioning, structured errors, message-shape cleanup, and richer snapshot semantics are introduced.

It covers:
- current wire message families
- current connection identification behavior
- current bounded-read semantics
- major producers and consumers
- current backward-compat assumptions already encoded in code/tests
- migration risks relevant to the hardening plan

## Current wire model

Transport:
- Unix sockets
- 4-byte big-endian length prefix + JSON payload
- max frame size: 10 MB
- no protocol version field in the frame
- no handshake or capability negotiation

Primary protocol enums (`crates/protocol/src/message.rs`):
- `AppMessage`
- `PluginMessage`
- `AppNotification`
- `PluginCommand`

Identifiers:
- entities are identified by `Urn`
- URN format: `{plugin}/{entity-type}/{id}[/{entity-type}/{id}]*`
- nested entities exist and are already used in protocol entity docs

## Message inventory

| Message family | Variants | Producer | Consumer | Bounded or streaming | Typed vs dynamic | Notes |
|---|---|---|---|---|---|---|
| `AppMessage` | `Subscribe`, `Unsubscribe` | apps / CLI | daemon | streaming control | typed except `params` elsewhere | triggers plugin startup via subscription |
| `AppMessage` | `Status` | apps / CLI | daemon | bounded query request | typed | asks daemon for cached entities by entity type |
| `AppMessage` | `TriggerAction` | apps / CLI | daemon | request/response | dynamic `params: serde_json::Value` | action result correlated by `action_id` |
| `AppMessage` | `Describe` | apps / CLI | daemon | bounded query request | typed | returns plugin descriptions |
| `PluginMessage` | `EntityUpdated` | plugin runtime | daemon | streaming event | dynamic `data: serde_json::Value` | duplicates `entity_type` alongside URN |
| `PluginMessage` | `EntityRemoved` | plugin runtime | daemon | streaming event | typed | duplicates `entity_type` alongside URN |
| `PluginMessage` | `ActionSuccess` | plugin runtime | daemon | request/response | optional dynamic `data` | compat path allows missing `data` field |
| `PluginMessage` | `ActionError` | plugin runtime | daemon | request/response | coarse `error: String` | no machine-readable code |
| `PluginMessage` | `StopResponse` | plugin runtime | daemon | bounded reply | typed | response to `CanStop` |
| `AppNotification` | `EntityUpdated` | daemon | apps / CLI | streaming event / bounded snapshot item | dynamic `data` | mirrors plugin update |
| `AppNotification` | `EntityRemoved` | daemon | apps / CLI | streaming event | typed | mirrors plugin removal |
| `AppNotification` | `ActionSuccess` | daemon | apps / CLI | request/response | optional dynamic `data` | correlated by `action_id` |
| `AppNotification` | `ActionError` | daemon | apps / CLI | request/response | coarse `error: String` | no structured semantics |
| `AppNotification` | `EntityStale`, `EntityOutdated` | daemon | apps / CLI | streaming lifecycle event | typed | duplicates `entity_type` alongside URN |
| `AppNotification` | `DescribeResponse` | daemon | apps / CLI | bounded query response | typed | one-shot response |
| `AppNotification` | `StatusComplete` | daemon | apps / CLI | bounded query delimiter | typed | current completion marker for `Status` |
| `PluginCommand` | `CanStop` | daemon | plugin runtime | bounded command | typed | graceful shutdown probe |
| `PluginCommand` | `TriggerAction` | daemon | plugin runtime | request/response | dynamic `params: serde_json::Value` | mirrors app action invocation |
| `PluginCommand` | `SubscriberCountChanged` | daemon | plugin runtime | streaming control | typed | lets plugin react to demand |

## Current connection identification behavior

Relevant code:
- `crates/waft/src/daemon.rs`

Current behavior:
- new connections begin as `ClientKind::Unknown`
- the daemon identifies the peer from the **first received frame**
- it tries `PluginMessage` first
- if plugin parse succeeds, the first message must be `EntityUpdated` or `EntityRemoved`
- plugin identity is derived from `urn.plugin()` in that first message
- otherwise it tries `AppMessage`
- if neither parse succeeds, the connection is rejected

Implications:
- there is no explicit peer-role handshake
- plugin startup semantics currently depend on sending an entity-bearing message first
- introducing a handshake is a protocol-shape change, not just a metadata addition

## Current bounded-read semantics

### `Status`

Relevant code:
- `crates/waft/src/daemon.rs`
- `crates/waft/src/query_command.rs`
- `crates/waft/src/commands_command.rs`

Current behavior:
- `AppMessage::Status { entity_type }` queries daemon cache only
- daemon emits zero or more `AppNotification::EntityUpdated` entries for cached entities of that type
- daemon then emits exactly one `AppNotification::StatusComplete { entity_type }`
- callers use `StatusComplete` as the bounded-read terminator

Current consumer behavior:
- `waft query` waits for `StatusComplete`
- `waft query --start` does `Subscribe` first, waits for live updates, then sends `Status`, then deduplicates by URN
- `waft commands` sends multiple `Status` requests and waits until all requested types emit `StatusComplete`
- `EntityStore` in GTK clients ignores `StatusComplete`

Implications:
- the protocol is not purely event-only anymore; it already has a bounded snapshot primitive
- completion semantics are explicit for `Status`, but only in a narrow, ad hoc way
- future snapshot work should evolve from this instead of pretending no completion model exists

## Current dynamic payload boundaries

Dynamic payloads today:
- `AppMessage::TriggerAction.params`
- `PluginCommand::TriggerAction.params`
- `PluginMessage::EntityUpdated.data`
- `PluginMessage::ActionSuccess.data`
- `AppNotification::EntityUpdated.data`
- `AppNotification::ActionSuccess.data`

Current contract source:
- static entity metadata lives in `waft-protocol` registry
- runtime plugin descriptions are available through `Describe`
- runtime action/entity payload validation is mostly by convention and consumer-side deserialization

Implications:
- contracts exist socially and in Rust types at the edges, but not as explicit wire-level schema authority
- action params/results are the easiest initial hardening target because they are smaller than all entity payloads

## Major producers and consumers

### Producers

- plugin runtimes via `waft-plugin`
  - `crates/plugin/src/runtime.rs`
- daemon
  - `crates/waft/src/daemon.rs`
- CLI app clients
  - `crates/waft/src/query_command.rs`
  - `crates/waft/src/commands_command.rs`
- GTK app clients through `waft-client`
  - `crates/client/src/connection.rs`

### Consumers

- daemon consumes `AppMessage` and `PluginMessage`
- plugins consume `PluginCommand`
- CLI and GTK apps consume `AppNotification`
- test harness uses raw framing directly
  - `crates/test-harness/src/app.rs`
  - `crates/test-harness/src/plugin.rs`

### Major higher-level consumers to keep in migration scope

- `waft-client` read/write wrappers: `crates/client/src/connection.rs`
- GTK entity cache and action callbacks: `crates/client/src/entity_store.rs`
- overview/settings/launcher via `WaftClient`
- CLI query path: `crates/waft/src/query_command.rs`
- CLI commands path: `crates/waft/src/commands_command.rs`
- daemon integration tests under `crates/waft/tests/`
- protocol serde tests in `crates/protocol/src/message.rs`
- docs/examples in `crates/waft/README.md` and `crates/test-harness`

## Current backward-compat assumptions already encoded

1. `ActionSuccess` without `data` is accepted.
   - `crates/protocol/src/message.rs`
   - both plugin->daemon and daemon->app compatibility tests already assert this

2. First-message peer identification is shape-based.
   - `crates/waft/src/daemon.rs`
   - a handshake cannot simply be inserted without a transition path

3. `Status` is a cache query with explicit completion.
   - `crates/waft/src/query_command.rs`
   - `crates/waft/src/commands_command.rs`

4. `entity_type` is treated as routing data, not just redundant decoration.
   - daemon subscriber fanout currently keys off the explicit `entity_type` field in entity messages
   - GTK entity store caches and groups by the explicit `entity_type` field

5. Raw length-prefixed JSON framing is part of the compatibility baseline.
   - `crates/protocol/src/transport.rs`
   - `crates/test-harness/src/app.rs`
   - `crates/test-harness/src/plugin.rs`

## Migration risks by concern

### 1. Versioning / capability negotiation

Risk: high

Why:
- no version field exists today
- no peer-role handshake exists today
- first-frame classification is structurally incompatible with a mandatory hello unless daemon and runtimes gain a dual-path transition

### 2. Dynamic payloads

Risk: medium

Why:
- many consumers currently deserialize directly from `serde_json::Value`
- action params/results are manageable first targets
- entity payloads are broader and should stay staged

### 3. `entity_type` redundancy

Risk: medium-high

Why:
- redundancy is real, but current routing and UI code rely on the explicit field
- nested URNs require a clear rule: root entity type vs leaf entity type
- without a fixed rule, cleanup would be ambiguous and risky

### 4. Event-centric protocol shape

Risk: medium

Why:
- a bounded read primitive already exists (`Status` + `StatusComplete`)
- the real work is to formalize and generalize it, not invent a wholly new concept
- query/bootstrap consumers can hang or misread results if completion ordering changes carelessly

### 5. Structured errors

Risk: medium

Why:
- current action errors are strings end to end
- clients log/display strings directly
- changing the wire shape needs compatibility decoding and UI/CLI formatting updates

## Phase 0 decisions fixed enough for ADR drafting and curation

These are stable enough to move into ADRs:
- keep length-prefixed JSON transport
- add compatibility-first handshake instead of flag-day protocol replacement
- keep `Status` as the starting point for bounded snapshot semantics
- prioritize action params/results before entity payload typing
- treat structured errors as additive first, then migratory
- resolve entity message `entity_type` from URN only after a canonical leaf/root rule is fixed

## Open questions handed to ADRs

1. Should handshake negotiation use a single version, a min/max range, or epoch + capability set?
2. When does handshake become mandatory for built-in peers?
3. Which schemas are authoritative in the daemon versus only descriptive?
4. For nested URNs, is canonical message entity type the leaf entity type?
5. Should structured protocol errors carry localization hooks, or only stable codes plus English developer text?
6. Should richer snapshot semantics extend `Status` or introduce a new query message family later?

## Remaining curation constraints before Phase 1+

Phase 0 is not complete until the ADR set fixes at least:
- handshake first-frame discrimination — resolved by ADR 0001
- plugin handshake identity versus implementation identity — resolved by ADR 0001
- duplicate `Status` request semantics — resolved by ADR 0003
- snapshot + stream duplication semantics — resolved by ADR 0003
- action error wire transition strategy — resolved by ADR 0004
- legacy-versus-negotiated mismatch behavior for derived entity types — resolved by ADR 0005
- first-slice schema representation — resolved by ADR 0002

## Phase 0 completion checklist

- [x] current protocol behavior documented
- [x] migration surface enumerated
- [x] protocol inventory table written
- [x] existing compatibility assumptions captured
- [x] baseline is ready to drive ADR drafting and curation
