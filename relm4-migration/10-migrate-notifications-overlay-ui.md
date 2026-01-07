# 10 — Migrate Notifications Overlay List UI to Relm4 (Backed by Domain Core)

## Goal

Migrate the **Notifications overlay list UI** (the in-overlay notifications center) to a **Relm4 + libadwaita (`adw`) component** backed by the **UI-agnostic notifications domain core** created in step 09, while keeping:

- app buildable at the end of the step,
- all automated tests passing,
- DBus behavior correct (as established in step 07),
- UI updates incremental and responsive (no “rebuild everything” loops).

**Explicitly out of scope for this step:** the separate **toast window** behavior and its special lifecycle/geometry semantics. That comes in the next step.

## Scope

### Included
- A Notifications plugin overlay component that renders:
  - a list of notifications,
  - per-notification actions (buttons),
  - dismiss/clear controls (as applicable),
  - markup body rendering support (`body-markup`) consistent with capabilities.
- Wiring from DBus ingress → app router → notifications overlay component.
- Wiring from user interactions (dismiss/action clicks) → app/router → DBus signals + state updates, via the domain core.
- Fast automated tests:
  - heavy unit tests for reducers/logic (non-GTK),
  - optional integration tests for DBus signals if needed (no UI).

### Excluded
- Toast window / “toast popups while overlay hidden” behavior.
- Any UI-driver tests, pixel/layout assertions, screenshot testing.
- Plugin unload/reload behavior (plugins remain static).

## Changes (what you will do)

### A) Define the Notifications plugin surface in the new plugin framework

Ensure the Notifications plugin is represented as a Relm4 plugin (step 04 framework) providing an overlay component.

**Measurable outcome:** notifications no longer render via legacy GTK widget composition in the Relm4 overlay path; they mount as a Relm4 component.

### B) Component architecture: model, messages, and inputs

Implement a Relm4 component (e.g. `NotificationsOverlayComponent`) with:

#### Model (pure data, no GTK handles)
- `notifications: Vec<NotificationViewModel>` (or equivalent snapshot-derived items)
- Any view state:
  - expanded/collapsed (if supported),
  - selected/hovered state (optional),
  - “unread” indicator state (optional),
  - any transient UI state (e.g. “closing…”), but prefer to rely on core state + events.

**Important:** prefer keeping the component model as a projection of the domain core snapshot, to make “render from state” straightforward.

#### Messages
Split messages into:
1) **Incoming messages** from the app router / domain layer:
- `Msg::DomainEvent(NotificationsDomainEvent)` or `Msg::SetSnapshot(NotificationsSnapshot)` or both.
- `Msg::OverlayShown/Hidden` (optional; toast gating is later, but you may want to pause expensive updates while hidden—do not add polling).

2) **User interaction messages** (emitted by the component):
- `Msg::Dismiss { id }`
- `Msg::InvokeAction { id, action_key }`
- `Msg::ClearAll` (if supported)
- (Optional) `Msg::OpenSettings` etc.

#### Outputs to app/router
Do not let the component call DBus directly. Instead, on user actions emit app-level messages, e.g.:
- `AppMsg::Notifications(NotificationsAppRequest::Dismiss { id })`
- `AppMsg::Notifications(NotificationsAppRequest::InvokeAction { id, action_key })`

The app/router then:
- asks the domain core to apply the command,
- emits any DBus signals required,
- emits domain events back to the component (or a new snapshot).

**Measurable outcome:** user clicks produce app-level requests, not direct DBus calls from UI code.

### C) Rendering with libadwaita widgets (overlay list UI)

Render notifications using `adw`-friendly widgets. A typical structure:

- Section header row (e.g. `adw::ActionRow` or custom header):
  - Title “Notifications”
  - “Clear” button (if supported)
- List container:
  - `gtk::ListBox` (or `gtk::Box` with rows) for deterministic ordering
  - each notification row:
    - app/icon area (if available)
    - summary/title (required)
    - body text (supports markup; see below)
    - action buttons (if present)
    - dismiss button

**Markup rendering requirement (`body-markup`):**
- If current behavior renders markup via GTK label markup, preserve it.
- Make sure you treat the body string as markup only when appropriate and safely:
  - if you already assume DBus `body-markup`, keep that behavior consistent,
  - do not “sanitize” in a way that breaks existing client expectations unless explicitly desired in a future change.

**Measurable outcome:** `notify-send "Markup test" "<b>bold</b>"` still renders bold in the overlay list (manual smoke test).

### D) Incremental updates: avoid rebuilding the entire list repeatedly

Follow the project’s “incremental updates” guidance (avoid render → events → render loops):

1) Keep a stable mapping from `NotificationId` to row widgets/components OR use a Relm4 factory list.
2) On changes, update only:
   - a single row’s labels/buttons, or
   - add/remove one row.

Choose one approach and stick to it:

#### Option A (recommended): Factory component per notification
- Maintain a factory keyed by `NotificationId`.
- Add/remove/update items via messages.

#### Option B: Stable widget mapping (manual)
- Maintain `HashMap<NotificationId, NotificationRowWidgets>`
- On add: create row once and insert into list.
- On remove: remove row and delete from map.
- On update: set label text, buttons, sensitive state, etc.

**Ordering policy:**
- Preserve the existing ordering/grouping policy from step 09 (whatever the core snapshot provides).
- Do not reorder rows due to transient state changes unless the policy explicitly says so.

**Measurable outcome:** rapid bursts of notifications don’t cause UI freezes; updates do not flicker due to full list reconstruction.

### E) Wire DBus ingress → core → overlay UI (without GTK in DBus paths)

Use the step 07 pipeline and step 09 core:

- DBus interface receives `Notify` / `CloseNotification`.
- Domain core updates state, returns:
  - new id (for `Notify`),
  - domain events and/or a new snapshot.
- The app/router:
  - emits DBus signals (`NotificationClosed`, `ActionInvoked`) as required by policy,
  - forwards domain events/snapshot to the overlay component via typed message routing.

**Measurable outcome:** overlay list updates when notifications arrive via DBus.

### F) Add/extend automated tests (fast-first)

#### 1) Required unit tests (non-GTK)
Add tests that validate the UI-facing update logic without GTK:

- If you have a “projection” function `snapshot -> Vec<NotificationViewModel>`:
  - test mapping of summary/body/actions,
  - test stable ordering/grouping fields.
- If you have a reducer that applies domain events to a “view model store”:
  - `Add` adds a view model,
  - `Remove` removes,
  - `Update` updates only the targeted notification,
  - events are idempotent where appropriate.

These tests should not initialize GTK and should run in milliseconds.

#### 2) Optional integration tests (DBus-only; no UI)
If needed to protect regressions, extend step 07 DBus integration tests to validate that:
- after `Notify`, the app emits (or does not emit) expected signals,
- after `CloseNotification`, `NotificationClosed` is emitted with correct reason,
- action invocation path emits `ActionInvoked` then `NotificationClosed`.

Do not add UI-driven integration tests here.

### G) Minimal manual smoke tests (overlay notifications)

Because UI correctness is hard to fully automate, keep a short manual checklist:

1) Start app (Relm4 path).
2) Verify overlay shows Notifications section/list.
3) Run:
   - `notify-send "Overlay test" "Hello from notify-send"`
   - confirm it appears in overlay list.
4) Markup:
   - `notify-send "Markup test" "<b>bold</b> <i>italic</i>"`
   - confirm markup renders.
5) Action:
   - `notify-send --action=default=Open "Action test" "Click Open"`
   - click the action button in overlay list.
   - confirm it triggers the server-side action path (DBus signal emission is validated via DBus monitor below).
6) Close via DBus call:
   - monitor signals:
     - `dbus-monitor --session "type='signal',interface='org.freedesktop.Notifications'"`
   - use `gdbus call ... CloseNotification <ID>` (ID from monitor or logs)
   - confirm the notification disappears from overlay list and `NotificationClosed` signal is observed.

Note: if obtaining IDs manually is painful, temporarily log notification IDs on receive (track for removal in a cleanup step).

## Definition of Done (measurable)

- Notifications overlay list UI is implemented as a Relm4 + adw component in the new plugin framework.
- Overlay list updates in response to DBus notifications (via the core + router), without GTK calls from DBus threads.
- User actions in overlay list are routed through the app/router and domain core:
  - dismiss removes notification and emits correct close semantics,
  - action invocation emits `ActionInvoked` and closes notification per policy.
- Incremental updates are used (no full list rebuild loops on each event).
- Automated tests exist and pass:
  - new unit tests for snapshot/view-model mapping and/or event application logic,
  - existing core and DBus tests still pass.
- App remains buildable and tests pass at end of step:
  - `cargo build`
  - `cargo test`
  - plus the Relm4 path: `cargo build --features relm4-app` and `cargo test --features relm4-app` (or your chosen feature flag).

## Verification

### Build
- `cargo build`
- `cargo build --features relm4-app` (or your chosen Relm4 feature flag)

### Tests
- `cargo test`
- `cargo test --features relm4-app` (or equivalent)

### Manual smoke test (overlay notifications)
Run:
- `cargo run --features relm4-app`

Then:
- `notify-send "Overlay test" "Hello from notify-send"`
- `notify-send "Markup test" "<b>bold</b> <i>italic</i>"`
- `notify-send --action=default=Open "Action test" "Click Open"`

Monitor signals:
- `dbus-monitor --session "type='signal',interface='org.freedesktop.Notifications'"`

Confirm:
- overlay list shows notifications,
- markup renders,
- clicking action emits `ActionInvoked` and closes the notification (and you see `NotificationClosed`),
- `CloseNotification` removes from UI and emits `NotificationClosed` with correct reason.

## Notes / Guardrails

- Do not spawn background tasks from the component view construction; start tasks from explicit init/mount paths.
- Do not mutate GTK widgets from non-GTK threads. Use the app/router message channel.
- Avoid render-time side effects:
  - no “recompute snapshot by calling core” during view rendering in a way that can loop.
- Keep the overlay UI responsive:
  - no blocking in update handlers,
  - update only affected rows on events.
- Be explicit about any policy decision that affects external behavior:
  - unknown action behavior,
  - behavior for dismissing/closing unknown IDs,
  - replacement close reason.
  These should already be encoded and tested in step 09.

## Follow-ups (next steps preview)

- Step 11 will migrate the **toast window** to Relm4, including its special semantics:
  - toast window always mapped/visible until overlay is displayed,
  - zero height when there are zero notifications (blank),
  - toasts pop up when overlay is hidden,
  - overlay shown/hidden gating wired from the central router.
- Later steps remove remaining legacy GTK UI paths and make the Relm4 entrypoint the default.