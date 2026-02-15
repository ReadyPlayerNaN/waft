//! waft-settings - Standalone settings application for Waft.

mod app;
mod bluetooth;
mod display;
mod keyboard;
mod notifications;
mod pages;
mod sidebar;
mod weather;
mod wifi;
mod window;
mod wired;

use gtk::prelude::*;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    env_logger::init();

    let rt = tokio::runtime::Runtime::new()?;
    let gtk_app = rt.block_on(app::setup())?;

    let exit_code = gtk_app.run();
    std::process::exit(exit_code.into());
}
