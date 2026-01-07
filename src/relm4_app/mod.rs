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

// Step 03: new, GTK-free routing modules.
pub mod events;
pub mod router;

// Step 04: Relm4-first plugin framework (GTK-safe init vs mount boundary).
pub mod plugin_framework;
pub mod plugin_registry;

// Step 05: overlay host (window layout + mounting plugins).
pub mod overlay;

pub use app::run;

#[cfg(test)]
mod test_api {
    // The old step-02 skeleton types (`CoreModel`, `reduce`) were removed in step 05 when the
    // Relm4 app module became the overlay host. Keep test-only re-exports aligned with the
    // current structure:
    // - router/events remain GTK-free and are still used by unit tests
    pub(super) use super::events::{AppMsg, PluginId};
    pub(super) use super::router::{RouterEffect, RouterState, reduce_router};
}
