# ADR 0002: typed protocol envelopes and schema authority

## Status

Proposed

## Context

The current protocol uses `serde_json::Value` for:
- entity payload data
- action parameters
- action success payloads

This provides flexibility, but the wire contract is only partially explicit. Protocol metadata exists in two places:
- the static protocol registry in `waft-protocol`
- runtime plugin descriptions returned through `Describe`

Without a schema authority decision, typed evolution risks drift between docs, tests, plugins, and clients.

## Decision

Waft will distinguish between **typed protocol envelopes** and **schema-described domain payloads**.

### Canonical schema authority

Protocol-level schemas are canonical in the static protocol registry in `waft-protocol`.

That registry is the source of truth for:
- entity type names and URN patterns
- action names
- action parameter schemas
- action result schemas
- structured error code references for actions where applicable

Runtime plugin descriptions remain valuable, but they are projections of the same contract for:
- plugin-scoped discovery
- localized display strings
- runtime introspection

They must not become a competing protocol authority.

For the first hardening slice, runtime `Describe` responses should carry:
- existing human-readable action/property metadata
- embedded schema objects for action params and action results in new optional fields
- embedded entity data schema objects in new optional fields where available

The runtime description path should embed schema objects directly rather than only schema references, so Phase 2 has a single concrete projection target.

### Schema representation for the first slice

The first slice will use a **JSON Schema object subset** encoded directly in protocol metadata.

The supported subset should be sufficient for current action and entity contracts:
- `type`
- `properties`
- `required`
- `items`
- `enum`
- `description`
- `additionalProperties`

This keeps schema serialization concrete for tests without requiring the full JSON Schema feature surface on day one.

### Typing strategy

Waft will harden payload contracts in stages.

Stage 1:
- action parameter schemas become explicit
- action result schemas become explicit
- action success envelopes stay structurally stable but gain documented schema references

Stage 2:
- entity payloads remain data-driven at the wire level
- entity payload schemas become explicit in protocol metadata
- validation helpers may be introduced at plugin/client edges and in tests

### Validation policy

Initial validation should focus on:
- protocol metadata correctness
- action parameter/result compatibility in tests and selected boundaries

The daemon should not become a heavy semantic validator for every entity payload in the first slice.

## Consequences

### Positive

- action contracts become testable and discoverable
- static docs and runtime descriptions gain a clearer relationship
- entity payload typing can improve incrementally without a flag day

### Negative

- schema metadata must be maintained alongside existing Rust entity/action definitions
- some dynamic payloads remain during transition, which requires discipline rather than instant purity

## Implementation guidance

Phase 2 should begin with failing tests for:
- action schema serialization
- runtime description exposure of action schemas
- backward-compatible handling of retained dynamic payloads

The first implementation target is action params/results, not full entity payload static typing.

## Rejected alternatives

### Fully static typing for every entity payload immediately

Rejected because the migration surface is too wide and would entangle protocol cleanup with broad plugin churn.

### Runtime plugin descriptions as the only schema source

Rejected because protocol authority should not depend on plugin availability or localization projections.

### Leave everything as ad-hoc JSON with better docs only

Rejected because it does not improve machine-checkable compatibility.

## Deferred follow-up

- whether the supported JSON Schema subset should expand over time
- whether later protocol versions should switch runtime descriptions from embedded schemas to references plus canonical lookup
- how far daemon-side validation should eventually go
