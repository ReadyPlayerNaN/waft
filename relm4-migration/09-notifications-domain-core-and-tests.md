# 09 — Notifications Domain Core (UI-Agnostic) + Strong Unit Tests

## Goal

Extract and/or refactor the Notifications feature into a **UI-agnostic domain core** that can be driven by:
- DBus ingress (`org.freedesktop.Notifications`) and
- UI interactions (overlay list actions, toast clicks, dismissals)

…without depending on GTK/Relm4 types.

This step is about **correctness, testability, and stability**. It creates the foundation that later steps will use to render:
- the overlay notifications list, and
- the always-mapped toast window behavior.

At the end of this step:
- notification semantics live in pure Rust,
- unit tests cover the important rules,
- UI is not yet migrated (that happens in later steps).

## Scope

### Included
- A `notifications-core` module (in-tree) containing:
  - domain types (notification payload, icons, actions),
  - state store (in-memory), and
  - deterministic reducers/handlers for:
    - `Notify` (including `replaces_id` semantics),
    - close semantics (`CloseNotification`, dismiss, action-click close),
    - capability advertisement data.
- Strong, fast unit tests covering those semantics.
- If needed, minor wiring to ensure DBus ingress (from step 07) uses this core (still UI-free).

### Excluded
- Relm4 UI rendering of notifications (overlay list) — later step.
- Toast window rendering and its special lifecycle/geometry behavior — later step.
- Any UI tests.

## Changes (what you will do)

### A) Create a UI-agnostic core module

Create a dedicated module for notifications domain logic. Name it to fit your repo conventions; examples:
- `src/features/notifications/core/`
- `src/features/notifications/domain/`
- `src/notifications_core/`

Keep it **UI-free**:
- no `gtk`, no `adw`, no `relm4`, no `glib`.
- avoid pulling DBus-specific types into the core (use plain Rust types).

#### Suggested core structure (adjust as needed)
- `types.rs`
  - `NotificationId` (server-side id)
  - `Notification`
  - `NotificationAction` / action key
  - `NotificationIcon` / app icon representation
  - `CloseReason` (align with DBus reasons)
  - any “snapshot” types used by UI later (pure Rust)
- `store.rs` (or `state.rs`)
  - `NotificationsState` (in-memory store)
  - indexing for fast lookup
  - stable ordering/grouping policy (if applicable)
- `engine.rs` (or `reducer.rs`)
  - pure functions that apply events/commands to state:
    - `handle_notify(...) -> NotifyOutcome`
    - `handle_close_by_call(...) -> CloseOutcome`
    - `handle_dismiss_by_user(...) -> CloseOutcome`
    - `handle_action_invoked(...) -> ActionOutcome` (must also close per policy)
- `capabilities.rs`
  - returns the capability list advertised on DBus

> If you already have `types.rs`, `model.rs`, `controller.rs` etc., this step may reorganize: move semantics into the core and keep GTK/Relm4 pieces as thin adapters later.

### B) Define explicit inputs/outputs (commands + events)

Model your core around explicit inputs and outputs so it composes cleanly with DBus and UI.

Two common patterns; pick one and stick to it:

#### Pattern 1: Commands in, outcomes out
- Inputs are “commands”:
  - `NotifyCommand { app_name, replaces_id, summary, body, actions, hints, expire_timeout, ... }`
  - `CloseByCall { id }`
  - `DismissByUser { id }`
  - `InvokeAction { id, action_key }`
- Outputs are:
  - new IDs (for notify),
  - a list of domain events that external layers will translate to DBus signals and UI messages.

#### Pattern 2: Events-only reducer
- Inputs are events from DBus/UI.
- Reducer updates state and emits follow-up events.

Either is fine; **tests are easier** with Pattern 1.

**Measurable requirement:** The core returns enough information for external layers to:
- emit `ActionInvoked` and `NotificationClosed` signals,
- update UI models (add/remove/update notification items).

### C) Preserve required semantics (parity contract)

Ensure the core encodes these behaviors explicitly and deterministically:

1. **Server-side IDs**
- `Notify` returns server-generated notification IDs (> 0).
- IDs are unique over process lifetime.

2. **`replaces_id` semantics**
- When `replaces_id != 0`:
  - remove the old notification (if it exists),
  - create a new notification with a **new** ID,
  - the returned ID is the new ID.

3. **Close semantics**
- `CloseNotification` (DBus call) removes the notification and produces a close event with reason “closed by call”.
- Dismiss in UI removes and produces close event with reason “dismissed by user”.

4. **Action semantics**
- Clicking/invoking an action produces:
  - `ActionInvoked` event (with id + action key),
  - then closes the notification (producing `NotificationClosed` with the appropriate reason consistent with your policy).
- This ordering matters for tests and for DBus client expectations.

5. **Capabilities**
- Must include: `actions`, `body`, `body-markup`
- Must not include unsupported capabilities (e.g. persistence), unless implemented later.

> Note: markup rendering itself is UI, but advertising `body-markup` is a contract; the core should preserve body as a string and not strip markup.

### D) Keep DBus-specific mapping outside the core

Hints/actions/urgency/etc. may come in DBus shapes. Keep the conversion in the DBus ingress layer (step 07), and pass plain Rust equivalents into the core.

**Measurable:** `notifications-core` compiles with no dependency on zbus types.

### E) Add strong unit tests (heavy focus, fast)

Add a dedicated test module (or multiple) that executes purely in Rust.

#### Required unit tests
Minimum set (add more as you find edge cases):

1) **Capabilities**
- Returned list contains exactly (or at least) the required set:
  - includes: `actions`, `body`, `body-markup`
  - excludes: `persistence` and others not implemented.

2) **Notify returns ID and stores notification**
- `Notify` returns id > 0.
- State now contains exactly 1 notification under that id.

3) **Notify replacement**
- First `Notify` yields `id1`.
- Second `Notify` with `replaces_id = id1` yields `id2` where `id2 != id1`.
- State contains `id2` and does not contain `id1`.
- If your core emits close events for replacement, assert correct close reason for replaced removal (document your chosen reason; keep it consistent).

4) **Close by call**
- Create notification.
- Close by call removes it.
- Emits a close event with reason “closed by call”.
- Closing a non-existent id should be a no-op or a defined error — choose behavior, document it, and test it.

5) **Dismiss by user**
- Create notification.
- Dismiss removes it and emits “dismissed by user”.

6) **Invoke action emits ActionInvoked then closes**
- Create notification with actions.
- Invoke action emits:
  - `ActionInvoked(id, action_key)` event
  - then `NotificationClosed(id, reason=...)`
- State no longer contains the notification.
- If action key is unknown, define behavior (ignore? still close? error?) and test it.

7) **Ordering determinism (if you have grouping/sorting)**
If the app currently groups/sorts notifications (e.g. by timestamp, app, urgency):
- ensure insertion order or sort key order is deterministic and covered by tests.
- UI will later depend on stable ordering to avoid churn.

#### Testing rules
- Tests must not initialize GTK.
- Tests must not require a DBus bus.
- Tests should run fast (milliseconds), suitable for frequent iteration.

### F) Produce a “snapshot” API for UI consumption (pure Rust)

Introduce a method that returns a UI-friendly snapshot without UI types, e.g.:
- `fn snapshot(&self) -> NotificationsSnapshot`
- snapshot includes:
  - list of current notifications with display fields,
  - any computed grouping fields the UI needs.

This enables later UI steps to:
- render from snapshot,
- update incrementally based on emitted domain events.

Keep snapshot generation deterministic and unit-test it if it includes sorting/grouping.

## Definition of Done (measurable)

- A `notifications-core` module exists and is UI-free (no GTK/Relm4/glib imports).
- Core exposes:
  - command/event interfaces that encode notification semantics,
  - a way to obtain a snapshot for UI rendering later.
- Unit tests exist and pass for:
  - capabilities,
  - notify id generation,
  - replaces_id semantics,
  - close-by-call,
  - dismiss-by-user,
  - action-invoked ordering and closure policy,
  - any existing ordering/grouping rules (if applicable).
- The project remains buildable and tests pass:
  - `cargo build`
  - `cargo test`
  - and for the Relm4 path: `cargo build --features relm4-app` and `cargo test --features relm4-app` (or your chosen feature flag).

## Verification

### Build
- `cargo build`
- `cargo build --features relm4-app` (or your chosen Relm4 feature flag)

### Tests
- `cargo test`
- `cargo test --features relm4-app` (or equivalent)

### Manual smoke test (optional; not required for this step)
This step is primarily about domain core correctness via unit tests. If you want a sanity check without UI changes, you can:
- run the app (existing path),
- send a notification via `notify-send`,
- confirm DBus ingress still behaves (if wired through the core).

But the acceptance criteria should be satisfied by tests.

## Notes / Guardrails

- Do not let UI concerns leak into the core (no widget handles, no `glib` main context).
- Keep the core deterministic and side-effect free:
  - no spawning tasks,
  - no IO,
  - no DBus calls.
- Make policy decisions explicit:
  - replacement close reason,
  - unknown action behavior,
  - behavior on closing unknown IDs.
  Document these in the module docs and test them.
- Prefer small, pure functions: they are easier to test and refactor.

## Follow-ups (next steps preview)

- Step 10: Migrate Notifications overlay list UI into a Relm4 component using the new core snapshot + events.
- Step 11: Migrate Toast window behavior into Relm4:
  - toast window always visible until overlay is displayed,
  - zero height when no notifications,
  - toasts pop when overlay is hidden,
  - integrate overlay shown/hidden gating from the central router.
- Step 12+: Remove legacy GTK notifications UI and legacy widget-based plugin framework, make Relm4 the default entrypoint.