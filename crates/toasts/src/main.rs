//! waft-toasts - Standalone notification toast overlay application.

mod app;
mod toast_manager;
mod ui;

use gtk::prelude::*;
use waft_config::Config;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    env_logger::init();

    let config = Config::load();
    let rt = tokio::runtime::Runtime::new()?;
    let gtk_app = rt.block_on(app::setup(config.toasts.position))?;

    let exit_code = gtk_app.run();
    std::process::exit(exit_code.into());
}
