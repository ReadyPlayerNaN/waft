# 08 — Migrate Bluetooth Plugin Menu UI to Relm4 (Incremental List Updates + Tests)

## Goal

Migrate the Bluetooth plugin’s feature toggle **menu UI** to a real Relm4 + libadwaita (`adw`) component while preserving responsiveness and avoiding rebuild/flicker.

This step specifically validates that we can implement a **dynamic, DBus-driven list UI** in Relm4 using an incremental update strategy:
- stable per-device rows (no full rebuilds on every signal),
- only update affected widgets/rows when device state changes,
- keep background ingestion UI-free and feed updates through messages.

At the end of this step:
- the Bluetooth plugin no longer renders a placeholder/stub component,
- its UI is rendered in the overlay via Relm4,
- Bluetooth state changes propagate into the UI without polling,
- unit tests cover the core model/update logic without requiring GTK initialization.

## Scope

### Included
- Bluetooth plugin overlay tile/menu UI migrated to Relm4+adw.
- A Bluetooth **domain model snapshot** that represents adapter/device state for rendering.
- Incremental updates for device rows (factory list or stable mapping).
- Message routing:
  - App router → Bluetooth plugin component (incoming events)
  - Bluetooth plugin component → App router (user actions)
- Fast automated tests for model/reducer logic and event mapping.

### Excluded
- Notifications plugin migration (toast window + overlay list) — later steps.
- Full Bluetooth DBus backend rewrite (if already exists, adapt minimally; if not, stub ingress events is acceptable as long as model/update plumbing is correct and tested).
- UI test automation (pixel/layout assertions).

## Changes (what you will do)

### A) Define/confirm Bluetooth UI contract (what the menu must do)

Document (briefly, in code comments or in the plugin module docs) what the Bluetooth menu supports, for example:
- list of known devices (name, id/address, connected status),
- connect/disconnect toggles per device,
- adapter-level power toggle (if supported),
- transient states (connecting/disconnecting) if relevant,
- error display (optional but recommended).

**Measurable:** there is an explicit list of fields and actions the UI supports, with stable identifiers.

### B) Introduce a Bluetooth domain snapshot model (pure Rust)

Create/confirm a pure Rust model representing the “renderable” Bluetooth state:

- `AdapterSnapshot` (optional):
  - `powered: bool`
  - `available: bool` (or similar)
- `DeviceId`:
  - stable identifier (address/path). Must be hashable and comparable.
- `DeviceSnapshot`:
  - `id: DeviceId`
  - `name: String` (or `Option<String>`)
  - `connected: bool`
  - `paired: bool` (if relevant)
  - `trusted: bool` (optional)
  - `busy: bool` / `transition: Option<TransitionState>` (optional)
  - any additional UI-facing fields needed

The Bluetooth plugin should treat the DBus service (e.g. BlueZ) as the source of truth, but it must keep a local snapshot to render quickly and to support incremental updates.

**Guardrail:** This model must not contain GTK types.

### C) Define Bluetooth ingress events and mapping to `AppMsg`

Define a minimal event type that background tasks/DBus ingestion can emit, e.g.:

- `BluetoothEvent::AdapterChanged { powered: bool, ... }`
- `BluetoothEvent::DeviceAdded(DeviceSnapshot)`
- `BluetoothEvent::DeviceRemoved(DeviceId)`
- `BluetoothEvent::DeviceChanged { id: DeviceId, patch: DevicePatch }`

Then choose one of:
1. Map DBus → `AppMsg::ToPlugin { plugin: Bluetooth, msg: BluetoothMsg::… }`, or
2. Map DBus → `AppMsg::BluetoothEvent(BluetoothEvent)` and let the router forward.

Prefer (1) for clarity and type safety if your router already routes plugin messages.

**Measurable:** background ingestion code can emit an event without importing GTK/Relm4 UI types.

### D) Implement the Relm4 Bluetooth plugin component (menu + list)

#### 1) Component shape
Implement a Relm4 component for the Bluetooth overlay surface, likely:
- a “tile row” (or section) in the overlay, and
- an expandable “menu/details” area that contains the device list.

Use libadwaita widgets where appropriate:
- `adw::ActionRow` / `adw::PreferencesGroup`
- `adw::SwitchRow` for toggles (if it fits the UI)
- `gtk::ListBox` + row widgets if that’s already consistent with current UX
- Consider `adw::Clamp` / `adw::Bin` for layout polish if needed, but keep it minimal in this step.

#### 2) Incremental device list updates (no full rebuild)
Choose one incremental strategy and document it in the code:

**Option A (recommended): Relm4 factory components**
- Use a factory to manage a collection of `DeviceRow` components keyed by `DeviceId`.
- On `DeviceAdded`: insert one item.
- On `DeviceRemoved`: remove one item.
- On `DeviceChanged`: update that item only.

**Option B: Stable mapping of `DeviceId -> RowWidgets`**
- Keep a `HashMap<DeviceId, DeviceRowWidgets>` owned by the plugin component.
- Add/remove rows only when device set changes.
- Update widget properties only for affected rows on change events.

Given the project’s “React-ish incremental updates” guidance, either approach is acceptable as long as it:
- avoids reconstructing the entire list on every signal,
- keeps stable ordering (do not reorder rows on connect/disconnect),
- updates only the rows that changed.

**Measurable:** device connect/disconnect updates do not recreate all rows; only the target row updates.

#### 3) User actions -> messages (no direct DBus calls from view)
Add component messages for user actions:
- `BluetoothUiMsg::ToggleAdapterPower(bool)` (if applicable)
- `BluetoothUiMsg::ToggleDeviceConnection { id: DeviceId, connect: bool }`

The component update should:
- emit an app-level message requesting the operation (or call a domain controller that emits messages),
- optionally set a transient “busy/transition” state in the snapshot (so the UI reflects “connecting…”),
- rely on DBus events to confirm final state.

**Guardrail:** do not block the UI thread. Any async work must send results/events back via messages.

### E) Background ingestion (DBus) integration approach

If Bluetooth already has DBus ingestion code:
- refactor it to emit `BluetoothEvent` (or `BluetoothMsg`) into the central app router sender/sink.
- keep it Send-safe, UI-free.

If Bluetooth DBus ingestion is not yet present or is incomplete:
- you may keep the UI wired to a stub event stream for now, BUT:
  - the event types and reducer logic must be real and tested,
  - later steps must replace the stub with real DBus ingestion.

**Measurable:** there exists a single entrypoint “ingest Bluetooth events → AppMsg” used by the app startup.

### F) Add fast automated tests (heavy focus)

#### 1) Unit tests for the Bluetooth model/reducer (required)
Create unit tests that cover:
- `DeviceAdded` inserts a device without affecting unrelated devices.
- `DeviceRemoved` removes only the requested device.
- `DeviceChanged` updates only targeted fields and preserves ordering.
- User action message → produces correct “request” effect/message (connect/disconnect, power toggle).

These tests must not initialize GTK and should run fast.

#### 2) Unit tests for incremental update planning (required)
If you implement a “diff”/patch mechanism, test it:
- Given previous snapshot + new snapshot, diff produces:
  - `Add(id)` when new appears,
  - `Remove(id)` when missing,
  - `Update(id, patch)` when fields change,
  - `Noop` when identical.
- Ensure connect/disconnect does not reorder rows.

If you do not implement a diff function explicitly (factory approach), still test:
- deterministic “insert/remove/update” decisions based on incoming events.

#### 3) Integration-ish test for event routing (recommended)
Add a test that simulates:
- App router receives `BluetoothEvent::DeviceChanged { … }`
- It routes to Bluetooth plugin input as `BluetoothMsg::…`
- Bluetooth component reducer updates its model (pure test of reducer, not GTK).

Avoid UI rendering tests.

### G) Manual smoke tests (targeted, minimal)

Because Bluetooth UI is interactive and DBus-driven, keep manual checks short and focused on “does it work”:

1) Launch the Relm4 app.
2) Open overlay and locate the Bluetooth plugin menu.
3) Confirm device list renders (even if empty).
4) If you can use a real Bluetooth environment:
   - toggle a device connect/disconnect switch,
   - confirm UI shows a transient state (if implemented),
   - confirm final connected state updates when DBus reports it.
5) Confirm no flicker/rebuild:
   - device rows do not reorder on connect/disconnect,
   - only the targeted row changes.

If a real Bluetooth environment is not available, simulate events through a debug hook (temporary) that injects `BluetoothEvent`s and confirm row updates are incremental. Track any debug-only UI for removal in a later cleanup step.

## Definition of Done (measurable)

- Bluetooth plugin no longer uses a stub/placeholder overlay component; it is a real Relm4+adw component.
- Bluetooth plugin UI exposes its menu and a device list area (even if the list can be empty).
- Device list updates are incremental:
  - add/remove updates only affect the relevant rows,
  - connect/disconnect updates only affect the relevant row,
  - rows do not reorder on connect/disconnect.
- Background ingestion (real or stub) produces typed events/messages and does not import GTK types.
- Automated tests exist and pass:
  - unit tests for model and update logic (required),
  - unit tests for incremental update planning/diffing (required),
  - optional routing test (recommended).
- The app remains buildable and tests pass at the end of the step:
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

### Manual smoke test (Bluetooth)
Run:
- `cargo run --features relm4-app`

Confirm:
1. Overlay renders and Bluetooth component appears in the expected slot with expected ordering.
2. Opening the Bluetooth menu shows a device list section.
3. Trigger a device state change (real DBus or simulated):
   - only the relevant row updates (no full list flicker),
   - ordering remains stable.
4. Trigger connect/disconnect from UI:
   - no UI thread blocking,
   - state transitions behave sensibly and resolve on DBus event.

## Notes / Guardrails

- Do not poll on the GTK thread to “refresh devices”.
- Do not update GTK widgets from DBus/background threads.
- Treat the external Bluetooth service as the source of truth; local snapshot is for rendering + transitions.
- Prefer stable identifiers and stable ordering.
- Keep the UI update path bounded: apply only property changes to affected rows.
- If you add any debug-only event injection path, document it and schedule removal (later cleanup step).

## Follow-ups (next steps preview)

- Step 09: Begin migrating the Notifications overlay list component (non-toast) and associated pure models.
- Step 10: Migrate Notifications toast window behavior:
  - toast window always visible until overlay shown,
  - zero height when no notifications,
  - toasts pop when overlay hidden,
  - strong DBus + model tests and curated manual smoke tests.
- Step 11+: Remove legacy GTK widget-based plugin framework and make the Relm4 app the default entrypoint.