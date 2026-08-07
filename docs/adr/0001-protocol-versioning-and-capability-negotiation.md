# ADR 0001: protocol versioning and capability negotiation

## Status

Proposed

## Context

The current daemon protocol has no explicit version field, no peer-role handshake, and no capability negotiation. The daemon currently infers peer kind from the first message shape, and plugins must identify themselves by sending `EntityUpdated` or `EntityRemoved` first.

That makes future protocol evolution fragile:
- incompatible peers fail implicitly rather than negotiating explicitly
- new features cannot be gated safely by capability
- the first-frame classification rule blocks clean introduction of new wire semantics

## Decision

Waft will introduce an explicit connection handshake with negotiated protocol version and capabilities.

### Handshake model

Each new peer will begin with a handshake message that declares:
- peer role: `app` or `plugin`
- implementation identity: e.g. `waft-overview`, `waft-settings`, `waft-audio-daemon`
- for plugins, explicit **plugin name** matching the daemon registry key
- supported protocol version range: `min_version`, `max_version`
- advertised optional capabilities: string identifiers

The daemon will respond with either:
- handshake acceptance carrying the negotiated protocol version and enabled capability set, or
- handshake rejection carrying a structured protocol error

### First-frame parsing rule

Handshake is a distinct first-frame message family with a dedicated top-level `type` tag of `Hello`.

Unknown connections are classified by this rule:
1. inspect the first frame's top-level `type`
2. if it is `Hello`, parse handshake and negotiate explicitly
3. otherwise treat the peer as a **legacy peer with no negotiated capabilities** and fall back to legacy `PluginMessage`/`AppMessage` first-frame detection during the transition period

This avoids ambiguity with legacy messages because the existing protocol enums do not use `type: "Hello"`.

### Negotiation policy

- protocol versions are negotiated by **highest mutually supported version in the overlap range**
- if no overlap exists, the handshake is rejected deterministically
- capabilities are **additive and optional** unless explicitly marked required by a specific protocol version
- a feature that changes wire semantics must be gated by negotiated capability, negotiated version, or both

### Transitional rollout policy

Handshake rollout will be compatibility-first:
- during transition, the daemon will support both legacy first-message classification and the new handshake path
- all built-in app and plugin runtimes should migrate to the handshake path before legacy mode is removed
- once all maintained in-tree peers have migrated, handshake becomes mandatory for the current protocol version line

### Plugin identity policy

Plugin identity will no longer be inferred solely from `urn.plugin()` in the first entity-bearing message once handshake is active.
Instead:
- plugin role comes from the handshake
- plugin registry identity comes from the handshake's explicit **plugin name**
- implementation identity remains descriptive and may differ from plugin name
- entity URNs must remain internally consistent with the negotiated plugin name
- daemon validation should reject obvious plugin-name / URN plugin mismatches for negotiated peers

## Consequences

### Positive

- compatibility becomes explicit
- future protocol additions can roll out incrementally
- daemon can distinguish unsupported version from malformed message
- plugin/app identity becomes more deliberate and inspectable

### Negative

- daemon and runtimes need a dual-path transition period
- tests and tooling that assume first-frame entity identification must be updated
- test harnesses need handshake support once legacy mode is removed

## Implementation guidance

Phase 1 should start with failing tests for:
- exact version match
- overlapping version ranges
- no-overlap rejection
- capability absence for optional features
- legacy peer acceptance during transition

The first implementation slice should not remove legacy first-message support.

## Rejected alternatives

### No handshake, version only inside later messages

Rejected because it does not solve first-frame classification or capability negotiation.

### Capability-only negotiation without protocol versions

Rejected because it makes compatibility reasoning too implicit and weakens deterministic rejection.

### Immediate flag-day mandatory handshake

Rejected because the repository has multiple built-in clients, plugin runtimes, docs, and test harnesses that currently assume the legacy flow.

## Deferred follow-up

- exact wire shape of handshake request/accept/reject messages
- capability naming convention and registry location
- timeline for removing legacy first-frame mode
