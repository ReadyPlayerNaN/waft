# ADR 0004: structured protocol errors

## Status

Proposed

## Context

Current protocol failures are mostly string-shaped:
- `PluginMessage::ActionError { error: String }`
- `AppNotification::ActionError { error: String }`
- connection classification and compatibility failures are not yet structured at all

This is adequate for logs, but weak for programmatic handling and compatibility.

## Decision

Waft will introduce a structured protocol error shape and migrate failure paths to it incrementally.

### Error shape

Structured protocol errors should include at least:
- stable error `code`
- English developer-facing `message`
- optional machine-readable `details`
- `retryable` boolean
- error `scope` or `kind`

### Error taxonomy

The initial taxonomy should distinguish at least:
- transport / framing failure
- handshake incompatibility
- protocol validation failure
- capability not negotiated
- not found / unknown entity / unknown action / unknown plugin
- timeout / cancellation
- action execution failure

### Localization policy

The protocol error object is **not** responsible for localization in the first slice.
It should provide:
- stable machine-readable codes
- concise developer-facing English messages

UI layers may map codes to localized user-facing text later if needed.

### Rollout policy

- structured errors are additive first
- for action failures during transition, keep the existing `error: String` field and add an optional structured companion field such as `error_details`
- new decoders should prefer the structured companion when present and fall back to the legacy string otherwise
- old decoders continue reading the legacy string and ignore the added companion field
- handshake rejection may use structured errors directly because there is no legacy handshake consumer to preserve
- CLI and GUI consumers should preserve concise readable output while gaining code-based branching ability

## Consequences

### Positive

- clients can distinguish validation, permission, timeout, and compatibility failures
- handshake rejection becomes deterministic and inspectable
- action failure handling becomes more future-proof

### Negative

- all layers that currently log/display raw strings need migration touchups
- a bad taxonomy could overfit current needs if designed too narrowly

## Implementation guidance

Phase 3 should begin with failing tests for:
- structured error serde shape
- legacy compatibility decoding where retained
- retryable/details propagation
- action failure forwarding with structured semantics

The first adoption targets are handshake rejection and action failure paths.

## Rejected alternatives

### Keep string errors and add documentation only

Rejected because it does not provide machine-readable semantics.

### Make the protocol error object user-localized immediately

Rejected because localization belongs in UI/application policy, not the protocol core.

## Deferred follow-up

- exact code namespace conventions
- whether transport errors that never cross the socket should reuse the same code taxonomy or just map into it at reporting boundaries
