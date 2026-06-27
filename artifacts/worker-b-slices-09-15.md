Implemented slices 09-15.

Changed files:
- plugins/notifications/src/lib.rs
- crates/protocol/src/message.rs
- crates/client/src/connection.rs
- crates/client/src/entity_store.rs
- crates/plugin/src/lib.rs
- crates/plugin/src/plugin.rs
- crates/plugin/src/runtime.rs
- crates/overview/src/app.rs
- crates/waft/src/daemon.rs
- crates/waft/src/lib.rs
- crates/waft/src/main.rs
- crates/plugin/src/claim.rs (deleted)
- crates/waft/src/claim_tracker.rs (deleted)
- crates/toasts/ (deleted)
- Cargo.toml
- Cargo.lock
- README.md
- progress.md
- docs/plans/overview-toasts-merge-progress-tracker.md

Validation:
- `cargo build --workspace` passed.
- `cargo test --workspace` failed in unrelated `waft-plugin-claude` test (`credentials::tests::valid_token_returns_ok`).
- `cargo test -p waft-protocol` passed.
- `cargo test -p waft-client` passed.
- `cargo test -p waft-plugin` passed.
- `cargo test -p waft` passed.
- `rg -n "ClaimCheck|ClaimResponse|ClaimResult|ClaimSender|claim_tracker|send_claim_response|handle_claim_result|set_claim_sender" crates plugins` returned no matches.

Diff summary:
- Notifications `expire` now directly dismisses expired notifications and emits `NotificationClosed(EXPIRED)` without claim routing.
- Claim hooks/types were removed from plugin SDK, protocol, client, daemon, and overview.
- `crates/toasts` was deleted and workspace/docs were updated to reflect overview-hosted toasts.
- Progress tracking now records slices 09-14 complete and slice 15 blocked by unrelated workspace test failure.

Residual risks:
- Full workspace test suite is still blocked by the pre-existing `waft-plugin-claude` failure.
- I did not change D-Bus ownership; `plugins/notifications` still owns `org.freedesktop.Notifications`.
