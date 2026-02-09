use anyhow::Result;
mod app;
mod common;
mod daemon_widget_converter;
mod dbus;
mod features;
mod i18n;
mod menu_state;
mod plugin;
mod plugin_manager;
mod plugin_registry;
pub mod store;
mod ui;

// Re-export the set_field! macro from waft-core so feature plugins
// can continue to use `use crate::set_field;`.
pub use waft_core::set_field;

#[tokio::main]
async fn main() -> Result<()> {
    env_logger::init();
    app::run().await
}
