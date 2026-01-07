# 07 — DBus Ingress → `AppMsg` Pipeline (with Test Harnesses)

## Goal

Create a robust, testable **DBus ingress pipeline** that converts DBus activity into **typed `AppMsg`** messages handled by the central Relm4 app router, without touching GTK from DBus/background threads.

This step focuses on **architecture + test harnesses**:
- DBus server code (starting with `org.freedesktop.Notifications`) should emit **domain events**.
- Domain events should be transformed into **`AppMsg`** (or routed via a single narrow bridge).
- Unit + integration tests should validate semantics **without UI**.

No plugin UI is migrated in this step, but the notifications DBus ingress must become routable to the notifications plugin component in later steps.

## Scope

### Included
- A DBus ingress module that is independent of UI (no GTK/Relm4 types except `AppMsg`).
- DBus ownership policy enforcement (replace existing owner; fail startup if name cannot be acquired).
- Notification DBus interface ingress:
  - `Notify`
  - `CloseNotification`
  - `GetCapabilities`
  - (and any other methods you currently support; focus on those needed for parity contract)
- A test harness for DBus behavior:
  - **fast unit tests** for translation logic
  - **integration tests** for DBus protocol behavior on a temporary bus (recommended)

### Excluded
- Rendering notifications UI (overlay list or toast window).
- Any UI-level “toast window gating” behavior beyond emitting `AppMsg`.
- Bluetooth DBus (unless it’s already present and easy to adapt; otherwise leave for later).

## Changes (what you will do)

### A) Introduce a DBus ingress layer with a narrow surface

Create a module that represents DBus ingress in three explicit layers:

1) **DBus Interface Layer** (zbus-exposed)
- Implements `org.freedesktop.Notifications` methods/signals.
- Converts raw DBus calls into **domain-level ingress events**.
- Does not touch UI. Does not call GTK. Does not require Relm4.

2) **Domain Ingress Layer**
- Owns and updates a pure Rust “notification server model” (IDs, active notifications, replacement, close reasons).
- Produces domain events such as:
  - `NotificationReceived { … }`
  - `NotificationReplaced { old_id, new_id, … }` (or equivalent semantics)
  - `NotificationClosed { id, reason }`
  - `ActionInvoked { id, action_key }` (if you model this internally)
- This layer must be heavily unit-tested.

3) **Bridge Layer: Domain Event → `AppMsg`**
- Defines the mapping from domain/DBus events to `AppMsg` (router-facing) and then to plugin inputs via **typed handles** (Option 1.5A).
- Example (shape, not exact types):
  - DBus ingress: `NotificationReceived` → `AppMsg::NotificationsIngress(...)`
  - App wiring (post-reducer): `registry.get::<NotificationsSpec>() -> Option<PluginHandle<NotificationsSpec>>`
  - If present: `handle.send(&NotificationsInput::Add(notification))`
- This mapping must be deterministic and unit-tested:
  - unit tests for DBus/domain → `AppMsg` translation,
  - unit tests for the wiring behavior when the plugin is present vs absent (handle acquisition `Some`/`None`), without initializing GTK.

**Measurable outcome:** DBus code depends on a small interface like:

- `trait AppMsgSink { fn send(&self, msg: AppMsg) -> Result<(), SendError>; }`

or a channel sender type already used by your app/router.

### B) Standardize “no GTK from DBus threads” enforcement by design

Codify constraints in code structure:

- DBus interface methods must only:
  - parse inputs,
  - call into the domain ingress layer,
  - send `AppMsg` / domain events to the router via the sink.
- No widget references, no `glib` scheduling, no Relm4 component handles in DBus code.

**Optional (recommended):**
- Add doc comments and module-level notes stating:
  - “This module must remain UI-free; do not import GTK types here.”

### C) DBus name acquisition policy (must match parity contract)

Implement DBus startup such that:

- The app attempts to acquire/replace `org.freedesktop.Notifications` on the session bus.
- If acquisition fails, startup fails (the process exits / error returned from startup).

Split this into:
- a pure “policy” function that returns the desired acquisition options,
- a runtime function that attempts acquisition and reports structured errors.

This makes it testable (policy) and observable (runtime errors).

### D) Establish a test strategy with two tiers

#### Tier 1: Fast unit tests (required)

Add unit tests that do **not** spawn a bus and do **not** require async runtime beyond minimal.

Must cover:

1. **`GetCapabilities` correctness**
- Ensure returned capabilities match what you actually support (per parity contract / architecture notes):
  - must include: `actions`, `body`, `body-markup`
  - must not include unsupported capabilities (e.g. persistence)

2. **`Notify` translation & ID generation**
- `Notify` produces a new server-side ID.
- `Notify(replaces_id != 0)` semantics:
  - remove old notification
  - create a new notification (new ID)
  - emit appropriate domain event(s)

3. **`CloseNotification` semantics**
- closing an existing ID results in removal and a domain event with the correct reason (closed by call).

4. **Action invocation semantics (domain-level)**
- model that “invoking an action” triggers:
  - an `ActionInvoked` domain event
  - then a close (domain event) consistent with your policy (close after action click)

These can be tested purely against the domain ingress layer and event mapping layer.

#### Tier 2: DBus integration tests (recommended)

Add integration tests that run against an **isolated temporary session bus** so they are:
- deterministic
- do not require the user’s actual desktop notification daemon situation
- do not touch UI

If you already use `zbus`, a typical pattern is:
- start a private bus for the test process (or use a test helper that provides one),
- start the notification server interface on it,
- create a zbus client proxy and call methods,
- observe method results + signals.

Must cover at least:

1. **Name acquisition on the test bus**
- server can own `org.freedesktop.Notifications` on the test bus

2. **`Notify` returns an ID**
- calling `Notify` yields a numeric ID > 0

3. **`CloseNotification` emits `NotificationClosed`**
- subscribe to signals
- call `CloseNotification(id)`
- assert `NotificationClosed(id, reason=closed-by-call)` is emitted

4. **Action invoked emits `ActionInvoked` and closes**
- ensure when the action is invoked (depending on your API surface; either simulate via internal handler or via UI later):
  - `ActionInvoked` signal emitted
  - then `NotificationClosed` emitted

If simulating UI click is not feasible here, test the server-side method that the UI will call (if you have one) or test the internal “invoke action” function and verify emitted signals through the interface layer.

> Note: Avoid trying to verify visual behavior or GTK state in integration tests. Keep these tests DBus-only.

### E) Provide a minimal “fake sink” for tests

Implement a test helper type that collects sent `AppMsg`s:

- `CollectingSink` stores messages in a `Vec<AppMsg>` behind a mutex.
- Tests can assert:
  - message count
  - message variant shapes
  - ordering when relevant (e.g. action invoked then close)

This enables verifying the bridge layer without running Relm4.

### F) Wire into the app startup (but do not change UI)

Update the Relm4 app startup path to:
- construct the DBus server and pass it the `AppMsg` sink (sender),
- start DBus serving tasks in the background executor,
- route incoming DBus events to the router.

Keep the default/non-Relm4 path compiling until you later flip defaults; if both paths exist, you can:
- start DBus server in both, or
- start DBus server only in Relm4 path, but then ensure parity contract is clear about which build is authoritative during migration.

Prefer starting it in the Relm4 path only if you have feature-gated entrypoints.

## Definition of Done (measurable)

- A DBus ingress module exists with a clear split:
  - zbus interface layer
  - domain ingress/model layer
  - domain event → `AppMsg` bridge layer
- DBus code remains UI-free (no GTK usage).
- DBus name acquisition policy for `org.freedesktop.Notifications` is enforced:
  - attempt replace
  - fail startup if cannot acquire
- Unit tests exist and pass covering:
  - capabilities
  - Notify id & replacement semantics
  - CloseNotification semantics
  - action-invoked → close semantics at domain level
- Integration tests exist (recommended) and pass verifying:
  - method calls on a test bus
  - required signals (`NotificationClosed`, and `ActionInvoked` if feasible)
- The app remains buildable and tests pass:
  - `cargo build`
  - `cargo test`
  - plus the Relm4 path build/tests: `cargo build --features relm4-app` and `cargo test --features relm4-app` (or your chosen feature flag)

## Verification

### Build
- `cargo build`
- `cargo build --features relm4-app` (or your chosen Relm4 feature flag)

### Tests
- `cargo test`
- `cargo test --features relm4-app` (or equivalent)

### Manual smoke test (DBus-focused; no UI assertions)
On a real session bus (not the isolated test bus), run the app (Relm4 path if that’s where DBus server lives), then:

1. Verify name ownership:
- `busctl --user status org.freedesktop.Notifications`

2. Send a notification:
- `notify-send "Relm4 migration step 07" "DBus ingress check"`

3. If you have DBus signal monitoring:
- `dbus-monitor --session "type='signal',interface='org.freedesktop.Notifications'"`

Confirm you observe appropriate signals when closing a notification (either via client call or later UI).

> This smoke test is informational; correctness should primarily be guaranteed by unit/integration tests.

## Notes / Guardrails

- Keep DBus integration **zbus-only** (no mixed DBus stacks).
- Do not introduce GTK usage in DBus paths.
- Prefer modeling notification semantics in a testable pure Rust module; the zbus layer should be thin.
- Avoid polling; DBus is event-driven.
- Be explicit about close reasons and capability strings; they’re part of the external contract.

## Follow-ups (next steps preview)

- Step 08: Migrate Bluetooth plugin UI and (if needed) its DBus ingestion to the same “domain event → `AppMsg`” pattern.
- Step 09–10: Migrate Notifications plugin UI:
  - overlay list rendering (Relm4 component)
  - toast window rendering (Relm4 component + separate window)
  - overlay gating (toasts pop while overlay hidden)
  - preserve toast window semantics: always visible, zero height when empty
- Step 11+: Remove legacy GTK plugin/widget framework and make Relm4 the default entrypoint.