# Plan: daemon protocol hardening and evolution

## Status

Proposed phased TDD implementation plan.

## Goal

Address the main long-term weaknesses in the current daemon protocol without breaking Waft's good core properties:
- simple routing through the central daemon
- entity-first plugin model
- debuggable Unix-socket JSON transport
- gradual migration across many bundled plugins and clients

The protocol should remain pragmatic for desktop-shell use, while gaining:
- explicit protocol versioning and capability negotiation
- stronger contracts for payloads and errors
- cleaner message shapes
- a path from event-only delivery toward bounded state-sync semantics

## Triggering concerns

The current protocol has five structural concerns:

1. **No explicit protocol version / capability negotiation**
2. **Heavy use of dynamically typed payloads** (`serde_json::Value` for entity data, action params, action results)
3. **Redundant `entity_type` field** in messages that already carry a URN
4. **Event-centric protocol shape** with limited support for richer state-sync, replay, diffs, or transactions
5. **Coarse error model** (`String` errors) instead of structured machine-readable failures

## Plan philosophy

This plan intentionally avoids a flag-day rewrite.

Instead, it uses six rules:
- create and curate ADRs before broad plugin churn starts
- after ADR curation, continue as phased **TDD** work: red -> green -> refactor
- add compatibility layers before removing old shapes
- make protocol semantics explicit before making payloads stricter
- migrate one concern at a time with clear tests and CLI validation
- use local subagents deliberately for design pressure, review, and execution support

## Desired end state

At the end of this work, Waft should have:

- a documented **protocol version and capability handshake**
- a **compatibility matrix** for daemon, apps, and plugins
- **structured protocol envelopes** for actions and errors
- a clear rule for when **entity type is derived from URN** versus sent explicitly
- explicit **snapshot / completion / delta semantics** for query-style reads
- an ADR-backed roadmap for future extensions such as diffs, replay, and transactional batches

## Non-goals

This plan does **not** require:
- replacing JSON transport
- replacing URNs
- moving business logic from plugins into the daemon
- redesigning GTK client architecture
- introducing a general distributed-systems protocol beyond Waft's desktop-shell needs

## ADRs to create first

Before implementation starts, create an `docs/adr/` directory and add these ADRs:

1. **ADR: Waft protocol versioning and capability negotiation**
   - define compatibility policy
   - define handshake shape
   - define required vs optional capabilities
   - define daemon/app/plugin negotiation rules

2. **ADR: Typed protocol envelopes and schema authority**
   - define where entity/action/result schemas live
   - define how static protocol registry and runtime plugin descriptions relate
   - define which payloads remain dynamic and which become structured

3. **ADR: Status/snapshot/delta protocol semantics**
   - define event stream vs bounded snapshot behavior
   - define completion markers and snapshot boundaries
   - define future diff/replay direction

4. **ADR: Structured protocol errors**
   - define error code taxonomy
   - define retryability/user-facing hints
   - define transport/protocol/domain/action error separation

5. **ADR: Message-shape simplification and `entity_type` redundancy removal**
   - define whether `entity_type` remains canonical, derived, or transitional
   - define migration and compatibility behavior

These ADRs should be treated as explicit implementation gates for the later phases.

## ADR curation gate

The implementation flow is:

1. write the baseline analysis
2. draft the ADR set above
3. **curate** the ADRs until decisions are stable
4. only then continue with the remaining phases as a **phased TDD plan**

"Curated" here means:
- terminology is aligned with the current protocol model
- compatibility policy is explicit
- unresolved design forks are closed or deferred explicitly
- migration constraints are concrete enough to write failing tests first

No protocol churn should start before this gate is passed.

## Subagent-assisted execution model

Use local subagents throughout the plan:

- **oracle**
  - challenge protocol design choices before implementation starts
  - review ADR drafts for hidden compatibility traps and migration risk
  - sanity-check phase boundaries before cutting code

- **reviewer**
  - review each phase after the green step and before refactor completion
  - look for contract drift, missing test coverage, and migration hazards
  - review the final diff for docs/protocol/test coherence

- **delegate**
  - handle bounded implementation slices once the parent agent fixes the contract and acceptance criteria
  - useful for localized test additions, mechanical migrations, and compatibility shims
  - should not invent protocol semantics; it implements the already-curated ADR direction

Recommended cadence per phase:
1. parent agent writes/updates failing tests
2. delegate implements the smallest slice needed to pass
3. reviewer critiques the result
4. parent agent refactors and integrates follow-up fixes
5. oracle is used at phase boundaries or whenever a design fork appears

## Workstreams

This plan is split into five workstreams executed in order after the ADR curation gate, with some overlap after Phase 1.

---

## Phase 0 — Document the current protocol and freeze the baseline

Goal:
- create a precise baseline before changing semantics

### Tasks

1. Write a short protocol baseline document in `docs/analysis/` or extend existing protocol docs with:
   - current message enums
   - current first-message daemon/plugin detection behavior
   - current `Status` behavior
   - current action result behavior
   - current backward-compat assumptions already relied on in tests

2. Audit all current protocol consumers:
   - daemon
   - `waft-plugin`
   - overview/settings/launcher clients
   - CLI query/commands paths
   - test harnesses and docs examples

3. Add a protocol inventory table covering:
   - message type
   - producer
   - consumer
   - bounded or streaming
   - typed or `serde_json::Value`
   - backward-compat risk

4. Capture gaps discovered during the audit as implementation notes inside this plan or a sibling analysis doc.
5. Draft the ADR set listed above from the baseline and audit.
6. Run ADR curation with subagents:
   - **oracle** challenges protocol assumptions, compatibility policy, and migration risk
   - **reviewer** checks terminology, completeness, and internal consistency
7. Revise the ADRs until they satisfy the ADR curation gate.

### Deterministic verification

Red:
- baseline document and ADR drafts are incomplete until audit gaps are captured explicitly
- if an audit item has no owning consumer/producer entry, Phase 0 is not done

Green:
- baseline analysis document exists under `docs/analysis/`
- ADR drafts exist under `docs/adr/`
- protocol inventory table covers every current message enum family and major consumer class

Refactor verification:
- re-read baseline and ADR drafts for terminology consistency
- reviewer signoff confirms the ADR set is internally consistent
- oracle signoff confirms unresolved forks are either closed or explicitly deferred

Objective signoff checklist:
- [ ] baseline analysis written
- [ ] protocol inventory table written
- [ ] ADR set drafted
- [ ] reviewer curation pass completed
- [ ] oracle curation pass completed
- [ ] remaining phases can name concrete failing tests before coding

### Exit criteria

- current protocol behavior is documented
- migration surface is enumerated
- ADRs are drafted and curated
- later phases have enough fixed decisions to begin with failing tests

---

## Phase 1 — Add explicit protocol versioning and capability negotiation

Goal:
- make compatibility explicit before introducing stricter semantics
- do it as a TDD slice after the ADR curation gate

### Design direction

Introduce a small handshake layer at connection start.

The handshake should communicate at least:
- protocol version
- minimum supported version or compatible range
- advertised capabilities
- peer role (`app` or `plugin`)
- optional implementation identity (`waft-overview`, `waft-settings`, `waft-audio-daemon`, etc.)

### Tasks

1. Write failing protocol and daemon tests first for:
   - exact version match
   - older peer with subset capabilities
   - unsupported version rejection
   - optional capability absence
2. Extend protocol types with handshake messages or envelope fields.
3. Add daemon-side negotiation rules:
   - accept known-compatible peers
   - reject clearly incompatible peers with structured error details
   - allow capability-based feature rollout
4. Add plugin runtime support in `waft-plugin`.
5. Add client support in app/CLI connection code.
6. Refactor once tests pass, keeping compatibility-first behavior intact.

### Migration policy

- first ship the handshake in a **compatibility-first** mode
- keep current message bodies working after handshake succeeds
- only later gate new features behind negotiated capabilities

### Deterministic verification

Red:
- handshake compatibility tests fail before implementation
- at least one unsupported-version rejection test must fail for the pre-change code

Green:
- targeted protocol and daemon negotiation tests pass
- plugin runtime handshake tests pass
- client connection tests pass for the negotiated path

Refactor verification:
- rerun the full protocol and daemon test suites after cleanup
- reviewer confirms no feature path depends on un-negotiated new semantics
- oracle confirms the negotiated capability model still matches the ADR

Suggested commands:
- `cargo test -p waft-protocol`
- `cargo test -p waft`
- `cargo test -p waft-plugin`

Objective signoff checklist:
- [ ] failing handshake tests were added first
- [ ] compatible peers negotiate successfully
- [ ] incompatible peers fail deterministically
- [ ] optional capabilities are absence-safe
- [ ] reviewer pass completed
- [ ] oracle boundary review completed

### Exit criteria

- every new connection negotiates protocol compatibility explicitly
- future features can be enabled by capability instead of guessing from message shape

---

## Phase 2 — Define typed envelopes and schema authority

Goal:
- reduce unbounded dynamic payloads without losing plugin flexibility
- do it as a TDD slice grounded in the curated schema ADR

### Design direction

Do **not** try to fully statically type every entity payload across the whole runtime at once.

Instead split the problem:

1. **Protocol envelope typing**
   - action invocation
   - action success payloads
   - action errors
   - snapshot/delta metadata

2. **Schema authority for dynamic domain payloads**
   - entity payloads remain data-driven initially
   - schemas become explicit and discoverable
   - validation becomes possible at boundaries and in tests

### Tasks

1. Write failing tests first for schema serialization, action metadata exposure, and backward compatibility.
2. Define where schemas are canonical:
   - static protocol registry for compile-time protocol metadata
   - runtime plugin description for plugin-provided documentation/localization
   - optional JSON-schema-like representation for action params/results and entity data

3. Extend protocol description types so actions can describe:
   - parameter schema
   - result schema
   - error codes they may emit

4. Add optional validation helpers in `waft-plugin` and daemon tests.
5. Decide and document which message fields remain `serde_json::Value` temporarily and why.
6. Refactor after green to keep the schema surface minimal and coherent.

### Suggested incremental target

Start with **action params/results** before entity data, because they are:
- narrower in surface area
- easier to validate
- more important for compatibility and CLI ergonomics

### Deterministic verification

Red:
- schema serialization and action metadata tests fail before implementation
- at least one backward-compat schema decoding test fails on the pre-change code

Green:
- action schema metadata tests pass
- schema serialization golden tests pass
- backward-compat tests pass for legacy dynamic payload handling retained by the ADR

Refactor verification:
- rerun protocol tests and any daemon/plugin tests that consume action metadata
- reviewer confirms the canonical schema authority is reflected consistently in docs and types
- oracle confirms the staged typing plan did not drift into accidental full static typing

Suggested commands:
- `cargo test -p waft-protocol`
- `cargo test -p waft`
- `cargo test -p waft-plugin`

Objective signoff checklist:
- [ ] failing schema tests were added first
- [ ] canonical schema authority is encoded in protocol/docs
- [ ] action params/results have explicit schema support
- [ ] transitional `serde_json::Value` use is documented deliberately
- [ ] reviewer pass completed
- [ ] oracle boundary review completed

### Exit criteria

- schema authority is documented and encoded in protocol types
- action payloads have a clear path from ad-hoc JSON to validated contracts
- entity payload typing has a staged migration story rather than an all-or-nothing rewrite

---

## Phase 3 — Introduce structured protocol errors

Goal:
- replace string-only failures with machine-readable error semantics
- do it as a TDD slice grounded in the curated error ADR

### Design direction

Define a shared protocol error shape, for example with fields like:
- `code`
- `message`
- `details`
- `retryable`
- `scope` or `kind`

Error categories should distinguish at least:
- transport framing/IO failure
- handshake/version incompatibility
- protocol validation failure
- unknown entity/action/plugin
- action execution failure
- timeout/cancellation
- capability not negotiated

### Tasks

1. Write failing tests first for:
   - code stability
   - backward-compatible decoding from legacy string-only payloads if retained
   - retryability and details propagation
2. Add a structured error type in `waft-protocol`.
3. Use it first for:
   - action failures
   - handshake rejection
   - daemon command/query failures where applicable
4. Keep legacy string errors readable during transition.
5. Update CLI formatting so users still see concise human-readable output.
6. Refactor error helpers and formatting after green.

### Deterministic verification

Red:
- structured error roundtrip tests fail before implementation
- at least one legacy string-error compatibility test fails on the pre-change code if compatibility is retained

Green:
- protocol structured error tests pass
- daemon/action failure propagation tests pass
- CLI formatting tests pass for concise human-readable output with structured backing data

Refactor verification:
- rerun protocol, daemon, plugin, and CLI tests touching failure handling
- reviewer confirms errors are machine-readable without regressing user-facing readability
- oracle confirms the error taxonomy still matches the ADR and does not overfit one plugin

Suggested commands:
- `cargo test -p waft-protocol`
- `cargo test -p waft`
- `cargo test -p waft-plugin`

Objective signoff checklist:
- [ ] failing structured-error tests were added first
- [ ] stable error codes exist for the targeted categories
- [ ] legacy error compatibility path is explicit
- [ ] CLI/UI surfaces remain readable
- [ ] reviewer pass completed
- [ ] oracle boundary review completed

### Exit criteria

- new failure paths are machine-readable
- UIs and CLI can react differently to validation, permission, not-found, timeout, and incompatibility failures

---

## Phase 4 — Clarify message shapes and remove `entity_type` redundancy

Goal:
- simplify update/remove/status semantics without risky ambiguity
- do it as a TDD slice only after the message-shape ADR is curated

### Design direction

First decide the canonical rule in ADR form:
- either `entity_type` is derived from `urn.root_entity_type()` or `urn.entity_type()` depending on message semantics
- or `entity_type` remains explicit only where it carries information not recoverable from the URN

The likely direction is:
- **entity update/remove messages** derive entity type from URN and stop requiring a duplicate field
- **subscription/query APIs** may still accept explicit `entity_type` because that is the routing key

### Tasks

1. Write failing tests first for canonical entity-type derivation, nested-URN cases, and legacy compatibility.
2. Audit all places where the top-level `entity_type` field is read instead of deriving from URN.
3. Decide the canonical derivation rule for nested URNs.
4. Add compatibility decoding for old messages carrying both fields.
5. Update daemon routing/tests to use the canonical source consistently.
6. Remove redundant writes once all built-in clients/plugins are migrated.
7. Refactor to collapse duplicate helpers once the transition path is proven.

### Key risk to resolve

Nested URNs make this slightly subtle:
- subscription target may be root entity type
- actual entity instance may be leaf entity type

The ADR must resolve that distinction explicitly before code changes start.

### Deterministic verification

Red:
- canonical derivation tests for nested URNs fail before implementation
- compatibility tests for old messages carrying both fields fail on the pre-change code if new semantics are introduced

Green:
- message roundtrip tests pass with the new canonical rule
- daemon routing tests pass for root-vs-leaf nested URN cases
- compatibility decoding tests pass for transitional mixed peers

Refactor verification:
- rerun protocol and daemon suites after removing duplicate helpers or writes
- reviewer confirms canonical entity-type semantics are consistent across docs, code, and tests
- oracle confirms the chosen root/leaf rule is stable and migration-safe

Suggested commands:
- `cargo test -p waft-protocol`
- `cargo test -p waft`

Objective signoff checklist:
- [ ] failing derivation tests were added first
- [ ] canonical root/leaf rule is documented
- [ ] compatibility decoding works for transitional peers
- [ ] redundant writes/reads are reduced deliberately
- [ ] reviewer pass completed
- [ ] oracle boundary review completed

### Exit criteria

- canonical entity-type source is documented
- redundant fields are transitional instead of semantically required
- nested-URN behavior is unambiguous

---

## Phase 5 — Add bounded snapshot/state-sync semantics beside the event stream

Goal:
- keep the current event-driven model, but add first-class bounded read semantics and a path toward richer state sync
- do it as a TDD slice after the snapshot/status ADR is curated

### Design direction

Do not replace subscriptions.

Instead add explicit semantics for at least three read modes:
- **stream**: ongoing updates after subscribe
- **snapshot**: bounded current-state response with explicit completion
- **snapshot + stream**: initial snapshot followed by live deltas

This phase should build on the existing `StatusComplete` direction and turn it into a fully documented model.

### Tasks

1. Write failing tests first for:
   - empty snapshot
   - multi-entity snapshot
   - completion ordering
   - subscribe then snapshot
   - reconnect/resubscribe bootstrap behavior
2. Define and document read semantics:
   - what `Status` means
   - ordering between snapshot items and completion
   - how snapshot+stream interacts with subscribe
   - whether stale/outdated markers belong to snapshot responses

3. Update daemon/query paths so bounded reads do not rely on timeout or silence heuristics.
4. Decide which future extensions are planned but deferred:
   - diffs
   - replay cursors
   - transactional/batched multi-entity updates

5. Record deferred extensions in the ADR rather than half-implementing them now.
6. Refactor query/bootstrap paths after green so completion semantics stay centralized.

### Important scope rule

This phase should deliver **better state-sync semantics**, not a full event-sourcing system.

The minimum successful outcome is:
- bounded reads are explicit
- clients can bootstrap without timeout guesses
- protocol leaves room for later diff/replay work

### Deterministic verification

Red:
- snapshot completion tests fail before implementation
- at least one existing timeout/silence-based bounded-read path is shown failing the new deterministic completion expectations

Green:
- protocol snapshot completion tests pass
- daemon/query tests pass for empty, multi-entity, and subscribe-plus-snapshot cases
- client bootstrap tests pass where completion notifications must be ignored or handled explicitly

Refactor verification:
- rerun protocol, daemon, query, and client tests after centralizing completion logic
- reviewer confirms bounded reads no longer depend on silence heuristics
- oracle confirms the final shape matches the ADR and leaves clean extension points for deferred diffs/replay work

Suggested commands:
- `cargo test -p waft-protocol`
- `cargo test -p waft`
- `cargo test -p waft-client`

Objective signoff checklist:
- [ ] failing snapshot tests were added first
- [ ] bounded reads complete explicitly
- [ ] timeout/silence heuristics are removed from the targeted paths
- [ ] bootstrap/query consumers handle completion semantics correctly
- [ ] reviewer pass completed
- [ ] oracle boundary review completed

### Exit criteria

- query-like consumers stop inferring completion from silence
- snapshot semantics are explicit and tested
- future diff/replay/transaction work has a defined extension point

---

## Recommended implementation order inside the repository

1. protocol baseline analysis
2. `docs/adr/*` creation
3. ADR curation with oracle/reviewer feedback
4. Phase 1 TDD slice in `waft-protocol` + daemon + runtimes
5. Phase 2 TDD slice for schema authority and typed envelopes
6. Phase 3 TDD slice for structured errors
7. Phase 4 TDD slice for redundancy cleanup and field removal
8. Phase 5 TDD slice for query/snapshot semantic cleanup
9. broader schema validation helpers and plugin adoption passes

This order keeps design decisions ahead of churn and keeps implementation in phased red/green/refactor slices.

## Validation strategy

### Automated

- serde roundtrip tests in `crates/protocol/src/message.rs`
- daemon integration tests for routing, handshake, rejection, and snapshot completion
- plugin runtime tests for negotiation and action error propagation
- CLI tests for readable structured failures and non-timeout bounded reads
- each phase starts with failing tests and ends with a green suite plus a refactor pass

### Manual

- `waft query <entity-type>` against running daemon
- `waft query <entity-type> --start`
- `waft plugin describe <name>` to confirm schema/error metadata exposure remains coherent
- startup with mixed older/newer built binaries during migration if practical

### Subagent checkpoints

- use **oracle** after baseline analysis and after each ADR draft set is complete
- use **delegate** for bounded implementation slices after the parent agent fixes the contract and acceptance criteria
- use **reviewer** after each green step and once more before phase signoff
- phase signoff should include both passing tests and reviewer/oracle feedback where the phase changes protocol semantics

## Deliverables

At minimum this plan should produce:

- `docs/adr/` with the protocol ADR set listed above
- one protocol baseline analysis document
- protocol type updates in `waft-protocol`
- daemon/plugin/client migration patches
- phased TDD evidence for each implementation slice
- tests covering negotiation, structured errors, and snapshot completion semantics
- reviewer/oracle notes captured in commit messages, plan updates, or linked implementation notes

## Deferred questions for the ADRs

These should be answered in the ADRs before implementation crosses from Phase 1 into later phases:

1. Is compatibility expressed as a single integer version, semver-like range, or epoch + capability set?
2. Is handshake mandatory on every connection immediately, or only after a short transitional period?
3. Are action result schemas authoritative enough to validate in the daemon, or only at client/plugin edges?
4. For nested URNs, which entity type is canonical in update/remove semantics: root or leaf?
5. Do structured errors need localization hooks, or only stable codes plus English developer text?
6. Should snapshot semantics stay as enriched `Status`, or become a new query message family?

## Success criteria

This plan succeeds if Waft ends up with a protocol that is still simple to reason about, but no longer depends on implicit compatibility, ad-hoc JSON contracts, duplicate shape hints, timeout-based bounded reads, or string-only failures.