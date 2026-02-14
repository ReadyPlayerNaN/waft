//! waft-toasts - Standalone notification toast overlay application.

mod app;
mod toast_manager;
mod ui;
mod waft_client;

use gtk::prelude::*;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    env_logger::init();

    let rt = tokio::runtime::Runtime::new()?;
    let gtk_app = rt.block_on(app::setup())?;

    let exit_code = gtk_app.run();
    std::process::exit(exit_code.into());
}
