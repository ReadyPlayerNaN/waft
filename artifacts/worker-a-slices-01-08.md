Implemented slices 01-08.

Changed files: `docs/plans/overview-toasts-merge-semantics.md`, `crates/overview/src/features/mod.rs`, `crates/overview/src/features/toasts/mod.rs`, `crates/overview/src/features/toasts/toast_manager.rs`, `crates/overview/src/features/toasts/toast_window.rs`, `crates/overview/src/app.rs`, `crates/overview/src/ui/main_window.rs`, `crates/overview/src/ui/toast_style.css`, `progress.md`.

Validation: `cargo build -p waft-overview` passed; `cargo test -p waft-overview` passed; `rg` claim-leftover grep found no matches.

Open risks/questions: integrated toasts were build-validated but not Wayland-smoke-tested here; toast position currently uses config default in overview startup.

Recommended next step: run a live `notify-send` smoke test under the running overview/daemon session, then proceed to slice 09 only if parity is acceptable.