//! waft-launcher entry point.

use anyhow::Result;

fn main() -> Result<()> {
    env_logger::init();
    waft_launcher::app::run()
}
