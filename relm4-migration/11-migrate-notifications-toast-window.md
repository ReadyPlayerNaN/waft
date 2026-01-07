# 11 — Migrate Notifications Toast Window to Relm4 (Overlay Gating + Zero-Height Semantics)

## Goal

Migrate the Notifications “toast window” (a separate surface that pops up notifications while the main overlay is hidden) to **Relm4 + libadwaita (`adw`)**, preserving existing semantics:

- Toasts appear when the **main overlay is hidden**.
- The toast window is **always visible** until the main overlay is displayed.
- With **zero notifications**, the toast window is:
  - blank, and
  - **zero height** (effectively invisible but still mapped/visible).

This step also enforces the **overlay visibility → toast gating** rule through the central app router established earlier.

The app must remain buildable and all tests must pass at the end of the step.

---

## Scope

### Included
- Implement the toast window as a Relm4-managed window/surface (separate from the overlay).
- Wire overlay shown/hidden events to enable/disable toast display (“gating”).
- Implement and preserve “always visible until overlay displayed” semantics.
- Implement “zero height when empty” semantics.
- Route notification domain events/snapshots (from steps 07–10) into the toast window component.
- Add fast automated tests for:
  - gating behavior,
  - toast queue/display policy,
  - zero-height state derivation,
  - ordering and dismissal rules (as applicable).

### Excluded
- Pixel-perfect styling/animation parity (functionality first).
- UI-driver tests that require a running GTK main loop.
- Plugin unloading/reloading (plugins remain static).
- Changes to DBus semantics (must remain consistent with parity contract).

---

## Background / Current Behavior to Preserve

From the project requirements:

1. The toast window is **always visible** until the main overlay is displayed.
2. Toasts appear when the overlay is hidden (toast gating).
3. If there are no notifications:
   - toast window is blank
   - toast window height is **0**
4. When overlay becomes visible, the toast window should stop presenting toasts.

This implies two separate concepts:
- **Toast window mapping/visibility**: whether the window exists and is “visible/mapped”.
- **Toast content gating**: whether new toast presentations are allowed.

This step must preserve both semantics.

---

## Changes (what you will do)

### A) Define the toast window component contract

Create a dedicated Relm4 component (or module) for toast window management, e.g.:
- `NotificationsToastWindowComponent`

It should have:
- **Model**: pure Rust state describing current toast items and rendering state.
- **Input messages**: from app/router and from notification domain events.
- **Output messages**: user interaction requests back to the app/router (dismiss, invoke action).

The toast window should not call DBus directly; it should emit app-level requests which the router/core/DBus layer handles (consistent with step 10).

**Measurable outcome:** there is a single, well-defined message boundary between:
- app/router ↔ toast component

---

### B) Make overlay visibility gate toast presentation (central router enforced)

Use the central router behavior (from earlier steps) to drive a boolean gating flag:

- On `AppMsg::OverlayShown`:
  - set `toast_gating_enabled = false`
  - (policy decision) stop showing new toasts immediately
- On `AppMsg::OverlayHidden`:
  - set `toast_gating_enabled = true`

**Important:** This gating should control **presentation of toasts**, not whether the window exists. The window can remain mapped/visible but with zero height when empty.

**Measurable outcome:** there is a single source of truth for gating (router state), not ad-hoc checks scattered across UI code.

---

### C) Preserve “toast window always visible until overlay displayed”

Implement the toast window lifecycle policy explicitly. One pragmatic policy that matches the described behavior:

- On app startup:
  - create and present the toast window (mapped/visible)
- When overlay is shown:
  - hide the toast window (or keep it visible but ensure it cannot obstruct; match current behavior: “until the main overlay is displayed” implies toast window stops being present/visible then)
- When overlay is hidden again:
  - show/present the toast window again (mapped/visible)

However, you stated: “At the moment, the Toast window is always visible, until the main overlay is displayed.”
This suggests:
- before overlay ever shows, the toast window is always visible
- once overlay shows, toast window may be hidden (or at least stop being shown)
- subsequent behavior should be clarified by existing app behavior; for this step, implement:
  - overlay shown => toast window hidden
  - overlay hidden => toast window shown
unless the inventory/parity docs say otherwise.

**Measurable outcome:** a deterministic mapping policy exists and can be reasoned about from messages.

---

### D) Implement “zero height when empty” semantics

When there are no toasts to render (and/or no notifications requiring toast display), the toast window must become:
- blank, and
- zero height.

Implementation guidance (choose one, document it, and keep it stable):

#### Option 1: Explicit height request / content-driven natural height (recommended)
- Use a single root container inside the toast window.
- When empty:
  - remove/hide all content widgets, AND
  - set window default size height to 0 (or set child to not request height), AND
  - ensure the window remains “visible/mapped” according to lifecycle policy.
- When non-empty:
  - add content back and let natural height expand, OR set a small fixed height based on toast count.

#### Option 2: Use a revealer/collapsible container
- Wrap toast content in a `gtk::Revealer` (or equivalent) and collapse it to zero height when empty.
- Ensure the window doesn’t keep a minimum height due to margins/padding.

**Guardrail:** You must avoid a feedback loop where “rendering computes size” triggers messages that cause re-render repeatedly. Size changes should be strictly derived from model state.

**Measurable outcome:** when toast list becomes empty, the window height becomes 0 without requiring polling.

---

### E) Define toast selection / presentation policy

You need a small policy for what constitutes a “toast-worthy” notification and how many to show:

Minimum policy (safe defaults):
- When a new notification arrives and `toast_gating_enabled == true`:
  - enqueue it for toast display
- Display:
  - either the latest notification only, or a small stack (e.g. max 3)
- When user interacts:
  - dismiss => request close/dismiss through router/core
  - action click => request invoke action through router/core (which also closes per policy)

To keep this step small, pick one simple policy:
- show only the most recent toast (stack size = 1)
- auto-expire after N seconds (optional; if existing behavior has expiry, preserve it; otherwise skip expiry)

If you implement timers:
- timers must not touch GTK directly from background threads
- they should emit messages back into the toast component via Relm4 mechanisms.

**Measurable outcome:** toast window shows a deterministic toast(s) when notifications arrive while overlay hidden.

---

### F) Wire notification events/snapshots into the toast component

Using the existing architecture (step 07 DBus ingress + step 09 core + step 10 overlay component), the toast window should update based on:

- domain events (`NotificationReceived`, `NotificationClosed`, etc.), and/or
- full snapshots (`NotificationsSnapshot`)

Recommended approach:
- Use domain events for incremental updates (add/remove/update one toast).
- Use snapshot only as a recovery mechanism (e.g. on overlay hidden → force refresh).

**Measurable outcome:** receiving a `Notify` results in:
- DBus core state updated
- toast component receives a message that causes toast presentation (when gated on)

---

### G) Keep DBus/UI boundaries correct

When the user clicks a toast action:
- toast component emits an app-level request:
  - `InvokeAction { id, action_key }`
- app/router/core:
  - emits `ActionInvoked` signal
  - closes the notification (and emits `NotificationClosed`)

When the user dismisses a toast:
- toast component emits:
  - `DismissByUser { id }`
- app/router/core:
  - closes and emits `NotificationClosed` reason “dismissed by user”

**Measurable outcome:** all DBus signals remain correct and are exercised by tests where possible (DBus-only integration tests).

---

## Automated Tests (fast-first)

UI-driver tests are intentionally avoided. Instead, this step requires a set of pure/unit tests around policy and state transitions.

### 1) Toast gating reducer tests (required, pure)
Add unit tests that validate:
- `OverlayShown` disables gating (`toast_gating_enabled = false`)
- `OverlayHidden` enables gating (`toast_gating_enabled = true`)

If your router emits effects:
- assert the appropriate `SetToastGating { enabled: ... }` effect is produced

### 2) Toast window lifecycle policy tests (required, pure)
Add unit tests for a pure function, e.g.:
- `fn toast_window_visibility(overlay_visible: bool, startup_seen_overlay: bool, ...) -> ToastWindowVisibility`

At minimum:
- startup (overlay hidden) => toast window visible/mapped
- overlay shown => toast window hidden (or “not presented” per your policy)
- overlay hidden after shown => toast window visible/mapped (if you implement that behavior)

If you need a “first time overlay shown” nuance, encode it explicitly and test it.

### 3) Zero-height derivation tests (required, pure)
Add unit tests for a pure function:
- `fn desired_toast_window_height(toast_count: usize) -> u32` (or an enum like `Zero | Natural`)
- When `toast_count == 0` => height == 0
- When `toast_count > 0` => height > 0 (or “Natural”)

This ensures the semantic is enforced by state, not by incidental widget behavior.

### 4) Toast queue policy tests (required, pure)
Add unit tests that validate:
- when gating is enabled and a new notification arrives => toast is enqueued/shown
- when gating is disabled => new notifications do not appear as toasts (but remain in overlay list state)
- dismiss removes toast item
- close events remove toast item
- action click emits the correct app request (then core closes as per policy; core already tested in step 09)

### 5) DBus integration tests (optional but recommended)
If feasible in your existing DBus test harness:
- run server on isolated bus
- simulate a notify
- verify that the server emits correct signals on:
  - close by call
  - action invocation
This step’s toast UI won’t be directly tested here, but it protects the end-to-end DBus contract.

---

## Manual Smoke Tests (curated, minimal)

These are important because “zero height” and multi-window presentation are hard to validate without UI tests.

1) Start the app in the Relm4 path.
2) Ensure the overlay is hidden.
3) Observe the toast window:
   - it should be mapped/visible,
   - if there are no notifications, it should be blank and **zero height**.

4) Send a notification:
- `notify-send "Toast test" "This should appear as a toast"`

Confirm:
- a toast appears in the toast window while overlay is hidden.

5) Show the overlay (whatever your normal mechanism is).
Confirm:
- toast window stops presenting toasts (gating disabled),
- toast window is hidden/not presented (per your implemented policy).

6) While overlay is visible, send a notification:
- `notify-send "Overlay visible" "Should NOT toast now"`

Confirm:
- it appears in the overlay notifications list (from step 10),
- it does **not** appear as a toast.

7) Hide the overlay again.
Send:
- `notify-send "Overlay hidden again" "Should toast now"`

Confirm:
- it appears as a toast.

8) With `dbus-monitor`:
- `dbus-monitor --session "type='signal',interface='org.freedesktop.Notifications'"`

Trigger:
- send an action notification:
  - `notify-send --action=default=Open "Toast action test" "Click Open on the toast"`
- click action on toast

Confirm signals:
- `ActionInvoked` then `NotificationClosed` (per policy)

---

## Definition of Done (measurable)

- Toast window is implemented as a Relm4-managed separate window/surface.
- Overlay visibility drives toast gating through the central router:
  - overlay shown => gating disabled
  - overlay hidden => gating enabled
- Toast window lifecycle matches the described semantics:
  - always visible until overlay is displayed (and thereafter follows the defined policy)
- When there are zero notifications/toasts:
  - toast window is blank
  - toast window height is **0**
- Toasts appear when overlay is hidden and new notifications arrive via DBus.
- User interactions on toast (dismiss/action) route through the app/router/core:
  - dismiss => closes notification with correct reason
  - action => emits `ActionInvoked` and closes notification
- Automated tests exist and pass:
  - gating reducer tests
  - lifecycle policy tests
  - zero-height derivation tests
  - toast queue/policy tests
- The app remains buildable and tests pass:
  - `cargo build`
  - `cargo test`
  - and for the Relm4 path: `cargo build --features relm4-app` and `cargo test --features relm4-app` (or your chosen feature flag)

---

## Verification

### Build
- `cargo build`
- `cargo build --features relm4-app` (or your chosen Relm4 feature flag)

### Tests
- `cargo test`
- `cargo test --features relm4-app` (or equivalent)

### Manual smoke test
Run:
- `cargo run --features relm4-app`

Then perform the curated smoke steps above.

---

## Notes / Guardrails

- Do not mutate GTK widgets from DBus/background threads. DBus ingress must send messages to the router/component.
- Do not introduce polling loops to “resize” the toast window; height must be derived from model state changes.
- Avoid render-time side effects; do not schedule work from view construction that can loop.
- Keep the toast window policy explicit and testable with pure functions.
- If you add temporary debug UI/actions to simulate notifications, track them for removal in a later cleanup step.

---

## Follow-ups (next steps preview)

- Step 12: Remove remaining legacy GTK UI paths for notifications and plugins; ensure Relm4 is the primary UI.
- Step 13: Remove feature flags/dual entrypoints and make Relm4 the default `main` path.
- Step 14: Cleanup and consolidation:
  - delete stub components,
  - remove compatibility shims,
  - tighten module boundaries,
  - ensure documentation and parity contract are updated to reflect the new architecture.