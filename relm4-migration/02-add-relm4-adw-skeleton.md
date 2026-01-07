# 02 — Add Relm4 + libadwaita Dependencies and Minimal App Skeleton

## Goal

Introduce **Relm4** and **libadwaita (`adw`)** into the project and add a **minimal, running Relm4+adw app skeleton** that can be built and tested, without migrating existing UI yet.

This step is intentionally small: it establishes the new foundation and verifies the toolchain/dependencies work in this repository.

## Changes (what you will do)

### A) Add dependencies

1. Add Relm4 and libadwaita dependencies in `Cargo.toml`.
2. Ensure the GTK major version matches the project (Relm4 typically targets GTK4).
3. Keep existing GTK dependencies for now if they are still used by current UI; do not delete them in this step.

**Notes**
- Prefer enabling Relm4’s `adw` integration feature if available/appropriate.
- Keep features minimal to reduce compile time.

### B) Add a minimal Relm4+adw entrypoint (behind a feature flag)

Add a new optional entrypoint that runs a minimal Relm4 application, without touching the existing app yet.

Recommended pattern:
- Add a Cargo feature, e.g. `relm4-skeleton`.
- When the feature is enabled, `main` runs the Relm4 skeleton.
- When it is disabled, `main` runs the existing implementation unchanged.

This keeps the app buildable at all times and makes the migration incremental.

### C) Minimal UI structure

The skeleton should:
- initialize `adw::Application` (or Relm4’s `RelmApp` configured for adw),
- create a single window,
- render a trivial UI element (e.g. a label “Relm4 skeleton running”),
- cleanly exit.

No plugin system changes yet. No DBus changes yet.

### D) Add one fast automated test

Add a tiny unit test that confirms the skeleton feature compiles and that basic model/message types are wired.

Because GTK/Relm4 UI initialization in tests can be fragile/headless-dependent, keep this test **non-UI**:
- test that the new module compiles and that core types can be constructed,
- or test a pure function / model reducer used by the skeleton component.

Do **not** add UI-driver tests in this step.

## Suggested file/module layout (adjust to project conventions)

- `src/relm4_app/mod.rs`
  - new module namespace for Relm4 code
- `src/relm4_app/app.rs`
  - defines the top-level skeleton component (model + message enum)
- `src/main.rs`
  - selects between old main and skeleton main via Cargo feature

If the project already has a strong module structure for “app/ui”, place these accordingly, but keep it isolated from existing GTK UI for now.

## Definition of Done (measurable)

- `cargo build` succeeds (default features / existing app path).
- `cargo build --features relm4-skeleton` succeeds (Relm4+adw skeleton path).
- `cargo test` passes (default features).
- `cargo test --features relm4-skeleton` passes.
- Running the skeleton feature shows a window with a simple visible widget (label/button) and then behaves normally (no panics).

## Verification

### Build
- `cargo build`
- `cargo build --features relm4-skeleton`

### Tests
- `cargo test`
- `cargo test --features relm4-skeleton`

### Manual smoke test
Run the skeleton binary (how you run it depends on your project setup), e.g.:
- `cargo run --features relm4-skeleton`

Confirm:
- a window opens using libadwaita styling (as applicable),
- the UI contains the expected label/text,
- no runtime warnings/panics related to GTK init order.

## Notes / Guardrails

- Do **not** migrate the overlay UI, plugins, or DBus in this step.
- Do **not** remove or refactor existing GTK code yet.
- Keep the feature flag approach so every commit remains buildable and testable.
- If the repository uses `clippy`/`fmt`, keep the skeleton compliant; however, do not introduce new lint gates in this step.

## Follow-ups (next steps preview)

- Step 03 will typically introduce the central Relm4 “App message router” model/messages.
- Later steps will migrate the plugin framework to “plugins as Relm4 components”.
- Notifications (toast window + DBus server) will migrate after the foundational router/component wiring is stable.