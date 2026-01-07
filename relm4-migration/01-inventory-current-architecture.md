# 01 — Inventory Current Architecture (UI, Plugins, DBus, Tests)

## Goal

Create a concrete, versioned inventory of the current application architecture **before** starting the Relm4 + libadwaita migration. This reduces “unknown unknowns”, makes effort estimates reliable, and provides a baseline for parity checks.

This step produces **documentation and a checklist**, not behavior changes.

## Changes (what you will do)

1. Write an inventory doc of:
   - current UI surfaces (overlay window, toast window, any auxiliary windows),
   - plugin list and what each plugin provides,
   - DBus services owned/consumed and their threading model,
   - existing domain models and how UI is updated today,
   - test coverage and gaps.

2. Add a small “parity contract” list of behaviors we will preserve during migration.

3. (Optional but strongly recommended) Add a `make`-like set of developer commands in documentation (not necessarily a Makefile) describing how to:
   - build,
   - run,
   - run tests,
   - run manual DBus smoke tests.

## Deliverables (files to create/update)

Create the following new markdown files:

- `docs/relm4-migration/inventory/ui.md`
- `docs/relm4-migration/inventory/plugins.md`
- `docs/relm4-migration/inventory/dbus.md`
- `docs/relm4-migration/inventory/models.md`
- `docs/relm4-migration/inventory/tests.md`
- `docs/relm4-migration/parity-contract.md`

If a `docs/` folder doesn’t exist yet, create it. Keep the content concise but specific: prefer tables and bullet lists over prose.

### Required contents per file

#### `ui.md`
For each UI surface/window:
- Name (e.g. “Overlay window”, “Toast window”)
- Purpose
- Visibility lifecycle (when created, when shown/hidden)
- Inputs (what events/messages cause updates)
- Outputs (what user actions trigger domain changes)
- Any non-default GTK behavior (layer shell, always-on-top, size-to-content, etc.) if applicable

Include the **toast window semantics** explicitly:

- Toast window pops notifications while overlay is hidden.
- Toast window is always visible until overlay is displayed.
- With zero notifications, toast window is blank and **zero height**.

#### `plugins.md`
Table with one row per plugin:
- Plugin id/name
- UI surfaces it owns (overlay widget(s), toast window integration, menus)
- Domain responsibilities (DBus, device state, notification state, etc.)
- External dependencies (DBus names, services, binaries)
- Current “state ownership” pattern (where the source of truth is)

#### `dbus.md`
List:
- Well-known names owned by the app (confirm only `org.freedesktop.Notifications`)
- Object paths + interfaces exported
- Methods/signals handled and emitted (high level is fine; link to code locations)
- Ownership policy:
  - attempt replace on startup
  - exit if unable to own the name
- Threading/executor model today (where DBus tasks run and how UI updates are scheduled)

Include a manual smoke-test checklist for notifications DBus (you can base it on the project’s existing architecture notes).

#### `models.md`
For each major domain model:
- Module path
- Core types
- Update sources (DBus signals, user actions, timers)
- Consumers (UI views)
- Current test coverage (yes/no + where)

#### `tests.md`
Inventory:
- All current unit tests and integration tests (paths + what they cover)
- Gaps relevant to the migration:
  - notification grouping/sorting rules (if not already covered)
  - DBus notify/close/action semantics (if not covered)
  - overlay show/hide gating (if relevant)
- Proposed tests to add during migration (do not implement them in this step; just list)

#### `parity-contract.md`
A short, explicit list of behaviors to preserve during migration (unless explicitly changed later). Must include at least:
- DBus ownership policy for `org.freedesktop.Notifications`
- Notification action invocation emits `ActionInvoked` and closes the notification
- `CloseNotification` removes + emits `NotificationClosed`
- `replaces_id` semantics (remove old, create new)
- Toast window lifecycle semantics (as described above)
- Plugins are static after startup

## Definition of Done (measurable)

- All deliverable files listed above exist and are filled with **concrete** information (module paths, responsibilities, behavior notes).
- Each plugin and each UI surface is represented in the inventory.
- `parity-contract.md` is short (ideally < 2 pages) and unambiguous.

No functional code changes are required for this step.

## Verification

### Build
- `cargo build`

### Tests
- `cargo test`

### Manual smoke check (documentation quality)
- You (or another contributor) can answer these questions by reading the inventory:
  1. What windows exist and when are they visible?
  2. Which plugin owns the toast behavior?
  3. Which DBus name(s) does the app own and what is the acquisition policy?
  4. Where would you add a unit test for notification grouping?
  5. Where would you add an integration test for DBus `Notify` / `CloseNotification`?

If any answer requires “go grep the code” rather than consulting the docs you just wrote, the inventory is not done.

## Notes / Guardrails

- Keep this inventory **implementation-faithful**: link to code locations (module paths, file names) where possible.
- Do not propose new architecture here beyond the parity contract and test gap list; later steps will do the redesign.
- If you discover implicit behaviors (e.g. overlay show/hide affects toast window mapping), capture them explicitly in `ui.md` and `parity-contract.md`.