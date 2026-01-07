# Relm4 Migration — Progress Tracker

This folder contains a step-by-step migration plan to rewrite the project to use **Relm4 + libadwaita (`adw`)**, including the overlay UI, plugin framework, DBus services, and models — with a strong focus on **fast automated tests** and keeping the app **buildable with passing tests at the end of every step**.

## How to use this tracker

- Treat each step file as a **mini-spec + acceptance checklist**.
- Track progress by editing **this README**:
  - mark steps as `[x]` when complete,
  - add links to PRs/commits,
  - add short notes about deviations/decisions.

### Global rule (applies to every step)

A step is not “done” unless:
- the app is **buildable**, and
- **tests pass** (`cargo test`), and
- the step’s **Definition of Done** is satisfied.

---

## Step checklist

> Tip: keep the PR/commit link in the “Link” column for traceability.

| Step | Status | Link | Notes |
|---:|:---:|:---|:---|
| [00](./00-overview-and-goals.md) Overview & goals | [ ] |  |  |
| [01](./01-inventory-current-architecture.md) Inventory current architecture | [x] |  | Added architecture inventory docs under `docs/relm4-migration/` + parity contract (no behavior changes). |
| [02](./02-add-relm4-adw-skeleton.md) Add Relm4 + adw skeleton | [x] |  | Default feature now runs a minimal Relm4+adw skeleton (`relm4-skeleton`); legacy app remains available via `--no-default-features`. Added a fast non-UI unit test for a pure reducer (`CoreModel` + `reduce`). No DBus/plugin/notification semantics changes. |
| [03](./03-establish-app-router-and-events.md) Establish app router + event types | [x] |  | Added GTK-free `relm4_app::events` + `relm4_app::router` with `AppMsg`/`PluginId` and a pure `reduce_router` producing `RouterEffect`s. Decision (updated): router no longer defines `PluginMsg` / `AppMsg::ToPlugin`; plugin message typing lives in plugins and routing is done via typed handles from the plugin registry (Option 1.5A). Legacy `UiEvent` remains unchanged. Notifications ingress is domain-ish, DBus-free types. Added unit tests for overlay→toast gating (plugin routing tests moved to step 04 registry tests). |
| [04](./04-plugin-framework-as-relm4-components.md) Plugin framework as Relm4 components | [x] | https://github.com/just-paja/sacrebleui/commit/2f34d7c407bf8e5b6cd567f77082e7c1e952d215 | Decision: routing uses “Option 1.5A” typed handles (no centralized plugin endpoint/message enum). Each plugin owns its own typed `Input` enum + `PluginSpec`. The registry stores endpoints type-erased, but exposes `registry.get::<Spec>() -> Option<PluginHandle<Spec>>`; once acquired, `handle.send(&Spec::Input)` is compile-time typed. Router/types were updated accordingly: `PluginMsg` and `AppMsg::ToPlugin` were removed; plugin routing is performed by app wiring using typed handles. Framework enforces `init()` (GTK-free) vs `mount()` (post-GTK) boundary. Tests cover placement sorting and typed-handle acquisition + routing without initializing GTK. |
| [05](./05-relm4-overlay-window-layout.md) Relm4 overlay layout + mount plugin components | [x] | TODO | Implemented Relm4 overlay host window layout with Top/Left/Right placement areas. Preserved ordering semantics (heavier weight goes lower). Wired window map/unmap to `AppMsg::{OverlayShown, OverlayHidden}` and executed `RouterEffect::SetToastGating` via typed plugin handle targeting Notifications (stub spec for now, real notifications Relm4 component will replace later). Added GTK-free unit tests for placement bucketing and for router-effect → typed-handle wiring (present + missing plugin cases). Feature flag renamed to `relm4-app` and kept as default. |
| [06](./06-migrate-simple-plugins-to-relm4-components.md) Migrate simple plugins | [ ] |  |  |
| [07](./07-dbus-ingress-to-appmsg-with-tests.md) DBus ingress → `AppMsg` + tests | [ ] |  | DBus integration tests are **required** in this step (temporary/isolated bus). |
| [08](./08-migrate-bluetooth-plugin-menu.md) Migrate Bluetooth menu | [ ] |  |  |
| [09](./09-notifications-domain-core-and-tests.md) Notifications domain core + tests | [ ] |  |  |
| [10](./10-migrate-notifications-overlay-ui.md) Notifications overlay UI | [ ] |  |  |
| [11](./11-migrate-notifications-toast-window.md) Notifications toast window | [ ] |  |  |
| [12](./12-remove-legacy-gtk-paths-and-flip-default.md) Remove legacy GTK paths; flip default | [ ] |  |  |
| [13](./13-cleanup-docs-and-remove-migration-scaffolding.md) Cleanup & remove migration scaffolding | [ ] |  |  |

---

## Current focus

- **Active step:** `05`
- **Owner:** @you
- **Branch/PR:** TODO (add link to the PR/commit that landed step 05)
- **Current blockers:** none

---

## Quick links

- Migration steps live in this folder: `sacrebleui/relm4-migration/`
- Each step defines:
  - Goal
  - Changes
  - Definition of Done
  - Verification (build/tests/manual smoke tests)

---

## Agent prompt template (start a specific step)

Copy/paste the following prompt to an agent. Replace the placeholders.

### Prompt

```
You are an expert Rust/GTK engineer. You are working in the `sacrebleui` repo.

Task: Execute migration step NN: "<STEP_TITLE>".

Read and follow:
1) `sacrebleui/relm4-migration/README.md` (progress tracking + global rules)
2) `sacrebleui/relm4-migration/NN-<step-file-name>.md` (this step’s spec)
3) `sacrebleui/AGENTS.md` (must-follow architecture + GTK/threading/init boundaries)

Hard requirements:
- Keep the app buildable at the end of the step.
- All tests must pass at the end of the step (`cargo test`).
- Heavy focus on fast automated tests (unit tests/integration tests where feasible). Avoid UI-driver tests unless explicitly required by the step.
- Preserve required behaviors (especially DBus ownership policy and notification semantics) unless the step explicitly changes them.

Process requirements:
- Before implementing any behavioral change, identify and confirm key decisions described in the step file (especially around DBus ownership, threading/main-loop boundaries, public API/data model changes).
- Prefer small, reviewable commits. Do not do a large refactor without checkpoints.
- Do not create GTK widgets before GTK is initialized. Respect the `init()` vs `mount()` boundary described in `AGENTS.md`.

Deliverables for this step:
- Implement all items in the step’s “Definition of Done”.
- Add/extend tests specified by the step.
- Update `sacrebleui/relm4-migration/README.md`:
  - Mark the step as complete `[x]` when done,
  - Add PR/commit link,
  - Add brief notes on key decisions and any deviations.

When you finish:
- Provide a short summary of what changed.
- Provide exact commands to verify (build + tests).
- List any follow-up issues discovered (should be tracked as TODOs or new steps).
```

### What the agent should output at minimum
- A summary of changes
- How to verify: build command(s) and test command(s)
- Any manual smoke tests required by the step
- Any deviations from the step plan (with rationale)

---

## Notes / guardrails (project-specific reminders)

- Plugins are **static after startup** (no unload/reload requirement).
- DBus ownership policy for `org.freedesktop.Notifications` must remain:
  - attempt to replace existing owner,
  - fail startup if unable to acquire.
- Toast window semantics (must preserve during migration):
  - toasts pop while overlay is hidden,
  - toast window always visible until overlay is displayed,
  - when empty: blank and **zero height**.
- Prefer explicit message routing through a central app router.
- Keep DBus/domain layers UI-free; keep UI updates on the GTK thread via messages.