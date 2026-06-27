Implemented the toast countdown fix described in docs/plans/toast-timer-stuck-fix-plan.md.

Changed behavior:
- CountdownBarWidget now owns countdown state internally and no longer exposes atomic running/paused handles.
- Countdown progress is derived from monotonic elapsed time instead of fixed 60ms accumulation.
- Pause/resume now go through widget methods only and source creation is guarded to avoid duplicates.
- Stop marks the countdown inert and prevents late timeout emission.
- NotificationCard hover handling now calls bar.pause()/bar.resume() only.

Focused tests:
- Added CountdownState unit tests covering pause/resume timing, fraction derivation, and stop inertness.

Validation:
- cargo build -p waft-ui-gtk -p waft-overview
- cargo test -p waft-ui-gtk -p waft-overview
- cargo clippy -p waft-ui-gtk -p waft-overview --all-targets -- -D warnings

Notes:
- Clippy emitted existing clippy.toml warnings about unreachable gtk4 launcher paths, but the command completed successfully.
- No notification ownership or plugin lifecycle semantics were changed.
