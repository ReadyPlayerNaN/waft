use anyhow::Result;
mod app;
mod dbus;
mod features;
mod ipc;
mod plugin;
mod plugin_registry;
mod ui;

#[tokio::main]
async fn main() -> Result<()> {
    env_logger::init();
    app::run().await
}
