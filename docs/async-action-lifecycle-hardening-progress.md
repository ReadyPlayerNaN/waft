# Async action lifecycle hardening progress

Plan: `docs/plans/async-action-lifecycle-hardening-plan.md`

## Status

Implementation complete for the common action lifecycle, daemon routing, UI admission, entity publication, connection backpressure, cancellation, and monitored bus recovery paths.

## Completed

- [x] Semantic action admission for transition actions in the daemon and overview/settings clients.
- [x] Protocol capability and `PluginCommand::CancelAction` support.
- [x] Plugin runtime action abort handles and cancellation cleanup hooks.
- [x] Serialized plugin entity publication transactions.
- [x] Duplicate action-ID protection and per-entity transition admission.
- [x] Non-blocking daemon connection queues with priority control delivery.
- [x] Stalled subscriber eviction on output backpressure.
- [x] Non-blocking plugin activation failure path; no event-loop registration wait.
- [x] VPN, WiFi, wired, Bluetooth, and UI rapid-action regression coverage.
- [x] NetworkManager/BlueZ cancellation rollback and bus-monitor recovery, with expensive VPN/WiFi/tethering refresh work moved off the signal receive path.
- [x] GOA, Darkman, GSettings, Awww, and power monitor reconnection loops.
- [x] Reconnect clearing of overview/settings pending action state.
- [x] Reusable D-Bus monitor error budget to prevent error spinning.

## Validation

- `tools/run-gtk-tests-with-layer-shell-stub.sh cargo test --workspace` passed.
- `tools/run-gtk-tests-with-layer-shell-stub.sh cargo clippy --workspace --all-targets -- -D warnings` passed.
- `tools/run-gtk-tests-with-layer-shell-stub.sh cargo check --workspace` passed.
- `git diff --check` passed.

## Known environmental limitation

Regular GTK builds require the unavailable host package `gtk4-layer-shell-0.pc`; validation used the repository layer-shell stub.

## Reviewer notes

The final reviewer found no blockers. BlueZ now refreshes the full object tree and replaces the action-side D-Bus connection after reconnect, retries setup failures before monitoring, and reconciles again after match rules are installed.

## Residual operational risk

The exact production D-Bus failure that triggered the original VPN freeze was not fault-injected against a live NetworkManager instance. The lifecycle now has bounded daemon tracking, cancellation propagation, rollback hooks, and authoritative reconciliation paths.
