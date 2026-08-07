# ADR 0005: message-shape simplification and entity type derivation

## Status

Proposed

## Context

Current entity-bearing messages duplicate `entity_type` alongside `urn`:
- `PluginMessage::EntityUpdated`
- `PluginMessage::EntityRemoved`
- `AppNotification::EntityUpdated`
- `AppNotification::EntityRemoved`
- `AppNotification::EntityStale`
- `AppNotification::EntityOutdated`

This is redundant in principle, but the current code uses the explicit field for routing and UI grouping. Nested URNs make the cleanup non-trivial because a URN has both:
- a root entity type
- a leaf entity type

## Decision

For entity instance messages, the canonical entity type is the **leaf entity type**, i.e. `urn.entity_type()`.

### Canonical rule

The following messages should treat entity type as derived from the URN during the hardened protocol line:
- entity updated
- entity removed
- entity stale
- entity outdated

For nested URNs:
- routing and grouping semantics use the **leaf** entity type
- the root entity type remains relevant for hierarchy interpretation, not for subscriber fanout of the leaf entity instance

### Why leaf type

The existing subscription model is per entity type, and nested child entities such as Wi-Fi networks or Bluetooth devices are consumed by their own child entity type.
Using the root entity type as canonical for those messages would break fanout semantics.

### Transitional policy

- compatibility decoding should continue to accept old messages that carry both `urn` and explicit `entity_type`
- daemon internal routing/caching should derive canonical entity type from `urn.entity_type()`
- for negotiated new peers, an explicit-field mismatch is a protocol error and the message is rejected
- for legacy peers, an explicit-field mismatch is logged as a warning and the message is dropped rather than reinterpreted through the mismatched field
- once all maintained peers migrate, redundant writes should be removed

### What remains explicit

Request messages that target an entity class rather than an entity instance remain explicit:
- `Subscribe { entity_type }`
- `Unsubscribe { entity_type }`
- `Status { entity_type }`
- `StatusComplete { entity_type }`

Those are keyed by requested/routed entity type, not by a specific entity URN.

## Consequences

### Positive

- entity instance message shapes get simpler
- nested-URN ambiguity is resolved decisively
- migration can be validated by comparing explicit legacy field to derived leaf type

### Negative

- daemon, clients, and tests must stop treating explicit `entity_type` as independent truth for entity instance messages
- transitional validation adds temporary complexity

## Implementation guidance

Phase 4 should begin with failing tests for:
- simple URN leaf derivation
- nested URN leaf derivation
- compatibility decoding of old explicit-field messages
- mismatch rejection or warning behavior for transitional peers

Do not remove explicit fields from request/response messages keyed by class-level entity type.

## Rejected alternatives

### Use root entity type as canonical for all entity messages

Rejected because it conflicts with child-entity subscription semantics.

### Keep explicit `entity_type` forever despite URN redundancy

Rejected because it preserves duplicate truth sources and blocks cleaner protocol evolution.

## Deferred follow-up

- exact timeline for removing redundant serialized fields from entity instance messages
