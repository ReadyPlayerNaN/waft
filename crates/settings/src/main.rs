//! waft-settings - Standalone settings application for Waft.

mod app;
mod audio;
mod bluetooth;
mod display;
mod i18n;
mod keyboard;
mod keyboard_shortcuts;
mod notifications;
mod pages;
mod plugins;
mod search_index;
mod services;
mod search_results;
mod sidebar;
mod sounds;
mod startup;
mod wallpaper;
mod weather;
mod wifi;
mod window;
mod wired;

use clap::Parser;
use gtk::prelude::*;

#[derive(Parser)]
#[command(name = "waft-settings")]
struct Cli {
    /// Navigate to a specific settings page on startup.
    #[arg(long)]
    page: Option<String>,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();

    env_logger::init();

    let rt = tokio::runtime::Runtime::new()?;
    let gtk_app = rt.block_on(app::setup(cli.page))?;

    // Pass the real process arguments so GTK forwards them to the primary instance
    // via D-Bus when the app is already running.  The primary instance's
    // connect_command_line handler reads --page from these forwarded args.
    let exit_code = gtk_app.run();
    std::process::exit(exit_code.into());
}
