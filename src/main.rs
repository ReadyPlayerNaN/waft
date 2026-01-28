use anyhow::Result;
mod app;
mod config;
mod dbus;
mod features;
mod i18n;
mod ipc;
mod menu_state;
mod plugin;
mod plugin_registry;
mod runtime;
pub mod store;
mod ui;

#[tokio::main]
async fn main() -> Result<()> {
    env_logger::init();
    app::run().await
}
