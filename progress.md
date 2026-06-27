# Progress

## Status
Done

## Tasks
- Fixed stuck toast countdown ownership and timing in CountdownBarWidget.
- Updated NotificationCard hover to use widget API only.
- Added focused CountdownState tests.

## Files Changed
- crates/waft-ui-gtk/src/widgets/countdown_bar.rs
- crates/waft-ui-gtk/src/widgets/notification_card.rs
- progress.md

## Notes
Validation passed for cargo build -p waft-ui-gtk -p waft-overview, cargo test -p waft-ui-gtk -p waft-overview, and cargo clippy -p waft-ui-gtk -p waft-overview --all-targets -- -D warnings.
