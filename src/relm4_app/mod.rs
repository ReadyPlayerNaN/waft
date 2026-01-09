//! Relm4 + libadwaita migration app host (`relm4-app` feature).
//!
//! This module is the incremental migration target for the app's UI:
//! it will evolve from the initial skeleton into the real overlay host.
//!
//! Guardrails:
//! - Keep this module isolated from the legacy GTK overlay path (`--no-default-features`).
//! - Respect the init/mount boundary from `AGENTS.md`:
//!   - plugin `init()` must be GTK-free (runs before GTK is initialized),
//!   - widget construction happens during Relm4 mount/run (post-init boundary).
//! - Keep routing/reducers GTK-free and unit-testable.
//!
//! Migration step 03:
//! - Establish app-wide router/event types + a pure reducer (GTK-free).
//! - New code should use `AppMsg` (do not expand `UiEvent`; it is legacy-only).

// Keep `app` private so macro-generated widget types don't leak into public interfaces.
mod app;

pub mod channels;
pub mod plugin;
pub mod plugin_registry;
pub mod plugins;
pub mod ui;

pub use app::run;
