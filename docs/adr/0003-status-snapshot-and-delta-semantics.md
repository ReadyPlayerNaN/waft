# ADR 0003: status, snapshot, and delta semantics

## Status

Proposed

## Context

The protocol is often described as event-centric, but it already has a bounded-read mechanism:
- apps send `AppMessage::Status { entity_type }`
- daemon responds with zero or more `EntityUpdated`
- daemon finishes with `StatusComplete { entity_type }`

Clients such as `waft query` and `waft commands` already depend on that completion marker. The problem is not the absence of snapshot semantics; it is that the semantics are narrow and under-documented.

## Decision

Waft will treat `Status` as the canonical bounded snapshot request for the current protocol line.

### Semantics

For one `Status { entity_type }` request:
- the daemon responds with zero or more `EntityUpdated` notifications for cached entities of that entity type
- the daemon then emits exactly one `StatusComplete { entity_type }`
- `StatusComplete` is the deterministic end-of-snapshot marker for that request

Concurrency rule for the current protocol line:
- a connection must not issue more than one in-flight `Status` request for the **same entity type** before receiving `StatusComplete`
- multiple distinct entity types may be in flight concurrently
- if a client violates the same-entity-type rule in the hardened protocol line, the daemon rejects the second request as a protocol validation error and does not disturb the first in-flight snapshot
- a later protocol capability may add explicit request correlation, but that is deferred

### Entity type meaning

For snapshot items:
- the `entity_type` used to satisfy a `Status` request is the requested entity type
- entity instance notifications continue to identify actual entity instances by URN
- message-shape cleanup may later derive entity type from URN for update/remove events, but bounded snapshot completion stays keyed by requested entity type

### Streaming relationship

The protocol has three effective read modes:
- **stream**: `Subscribe` then consume live notifications
- **snapshot**: `Status` then stop at `StatusComplete`
- **snapshot + stream**: client combines `Subscribe` and `Status` during bootstrap or refresh flows

The current protocol line will keep `snapshot + stream` as client orchestration rather than inventing a new combined subscription primitive immediately.

Duplication and ordering rule for `snapshot + stream` during the current protocol line:
- duplicates by URN are **allowed** when live updates interleave with a bounded snapshot
- consumers should treat the stream as last-write-wins by URN
- `Status` is not an atomic snapshot boundary relative to concurrent live updates; it is a bounded cache read with explicit completion

### Deferred richer sync work

The following are explicitly deferred beyond the first hardening slice:
- diff payloads
- replay cursors
- transactional multi-entity batches
- a new generalized query envelope if `Status` enrichment proves insufficient later

## Consequences

### Positive

- current behavior becomes explicit instead of heuristic
- query/bootstrap consumers gain a stable contract to test against
- future state-sync work can extend an existing model rather than replacing it wholesale

### Negative

- `StatusComplete` remains part of the public compatibility surface
- clients that ignore completion notifications must continue to tolerate them safely

## Implementation guidance

Phase 5 should begin with failing tests for:
- empty snapshot completion
- multi-entity snapshot completion
- subscribe plus snapshot bootstrap ordering
- reconnect/bootstrap consumers tolerating completion notifications

The first implementation target is deterministic completion behavior, not diffs or replay.

## Rejected alternatives

### Pretend the protocol is stream-only and redesign snapshots from scratch

Rejected because it ignores existing `StatusComplete` behavior already used by CLI consumers.

### Replace `Status` immediately with a wholly new query message family

Rejected for the first hardening slice because it would create needless churn before handshake and error foundations are in place.

## Deferred follow-up

- whether a future capability should provide a server-side `snapshot + stream` one-shot bootstrap
- whether completion metadata eventually needs request correlation beyond `entity_type`
