use anyhow::Result;
use gtk::prelude::ApplicationExtManual;
use log::debug;
mod app;
mod calendar_selection;
mod common;
mod components;
mod dbus;
mod entity_store;
mod features;
mod i18n;
mod layout;
mod menu_state;
mod plugin;
mod plugin_registry;
pub mod store;
mod ui;
mod waft_client;

// Re-export the set_field! macro from waft-core so feature plugins
// can continue to use `use crate::set_field;`.
pub use waft_core::set_field;

fn main() -> Result<()> {
    env_logger::init();

    // Create tokio runtime — its worker threads are independent of main thread
    let rt = tokio::runtime::Runtime::new()?;

    // Phase 1: async setup (block_on returns once setup completes)
    let gtk_app = rt.block_on(app::setup())?;

    // Phase 2: GTK main loop on main thread.
    // Tokio workers keep running independently — no I/O driver starvation.
    debug!("Running main loop");
    gtk_app.run();
    debug!("Finished main loop");

    Ok(())
}
