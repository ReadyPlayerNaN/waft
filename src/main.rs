use anyhow::Result;
mod app;
mod config;
mod dbus;
mod features;
mod ipc;
mod plugin;
mod plugin_registry;
pub mod store;
mod ui;

#[tokio::main]
async fn main() -> Result<()> {
    env_logger::init();
    app::run().await
}
