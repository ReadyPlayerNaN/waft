use anyhow::{Context, Result};
use clap::Parser;
use std::fs;
use std::path::PathBuf;
use std::process::{Command, Child};
use std::env;

mod screenshot;
mod states;
mod markdown;

use screenshot::capture_widget_screenshot;
use states::WidgetStates;
use markdown::generate_markdown;

#[derive(Parser, Debug)]
#[command(name = "widget-docs")]
#[command(about = "Generate documentation with screenshots for waft-ui-gtk widgets")]
struct Args {
    /// Output directory for documentation
    #[arg(short, long, default_value = "docs/widgets")]
    output: PathBuf,

    /// Generate docs for a specific widget only
    #[arg(short, long)]
    widget: Option<String>,
}

fn main() -> Result<()> {
    let args = Args::parse();

    // Check for X11 display and start Xvfb if needed
    let (_xvfb_process, screenshots_enabled) = setup_display()?;

    // Initialize GTK
    gtk::init().context("Failed to initialize GTK")?;

    // Create output directory
    fs::create_dir_all(&args.output)
        .context(format!("Failed to create output directory: {:?}", args.output))?;

    // Get widget states to document
    let widget_states = WidgetStates::all();

    // Filter if specific widget requested
    let widgets_to_process: Vec<_> = if let Some(ref widget_name) = args.widget {
        widget_states
            .iter()
            .filter(|w| &w.name == widget_name)
            .collect()
    } else {
        widget_states.iter().collect()
    };

    if widgets_to_process.is_empty() {
        if let Some(ref widget_name) = args.widget {
            anyhow::bail!("Widget '{}' not found", widget_name);
        } else {
            anyhow::bail!("No widgets to document");
        }
    }

    println!("Generating documentation for {} widget(s)...", widgets_to_process.len());

    // Process each widget
    for widget_state in &widgets_to_process {
        println!("  Processing {}...", widget_state.name);

        let widget_dir = args.output.join(&widget_state.name);
        let screenshots_dir = widget_dir.join("screenshots");

        fs::create_dir_all(&screenshots_dir)
            .context(format!("Failed to create directory: {:?}", screenshots_dir))?;

        // Generate screenshots for each state (if X11 available)
        let mut state_info = Vec::new();
        for state in &widget_state.states {
            let screenshot_path = if screenshots_enabled {
                let output_path = screenshots_dir.join(format!("{}.png", state.filename));

                println!("    Capturing state: {}...", state.name);
                capture_widget_screenshot(&state.widget, &output_path)
                    .context(format!("Failed to capture screenshot for state: {}", state.name))?;

                Some(format!("screenshots/{}.png", state.filename))
            } else {
                println!("    Skipping screenshot for state: {} (X11 not available)", state.name);
                None
            };

            state_info.push((state.name.clone(), state.description.clone(), screenshot_path));
        }

        // Generate markdown
        let readme_path = widget_dir.join("README.md");
        let markdown_content = generate_markdown(&widget_state.name, &widget_state.description, &state_info, screenshots_enabled);
        fs::write(&readme_path, markdown_content)
            .context(format!("Failed to write README: {:?}", readme_path))?;

        if screenshots_enabled {
            println!("    Generated {} screenshots and README.md", state_info.len());
        } else {
            println!("    Generated README.md (without screenshots)", );
        }
    }

    // Generate index page
    let index_path = args.output.join("README.md");
    let index_content = generate_index(&widgets_to_process);
    fs::write(&index_path, index_content)
        .context(format!("Failed to write index: {:?}", index_path))?;

    println!("\nDocumentation generated successfully at {:?}", args.output);

    // Exit immediately to avoid GTK renderer dispose assertion failures
    // This is a workaround for a GTK4 CairoRenderer bug
    std::process::exit(0);
}

/// Check for X11 display and start Xvfb if needed
/// Returns (optional Xvfb process, screenshots_enabled flag)
fn setup_display() -> Result<(Option<Child>, bool)> {
    // Check if DISPLAY is already set
    if env::var("DISPLAY").is_ok() {
        println!("Using existing X11 display");
        return Ok((None, true));
    }

    println!("No DISPLAY set, attempting to start Xvfb...");

    // Try to start Xvfb
    match Command::new("Xvfb")
        .args(&[":99", "-screen", "0", "1024x768x24", "-nolisten", "tcp"])
        .spawn()
    {
        Ok(mut process) => {
            // Set DISPLAY environment variable
            unsafe { env::set_var("DISPLAY", ":99"); }

            // Wait a moment for Xvfb to start
            std::thread::sleep(std::time::Duration::from_millis(500));

            // Check if Xvfb is still running
            match process.try_wait() {
                Ok(Some(status)) => {
                    eprintln!("Xvfb exited immediately with status: {}", status);
                    eprintln!("Screenshots will be disabled");
                    Ok((None, false))
                }
                Ok(None) => {
                    println!("Xvfb started successfully on :99");
                    Ok((Some(process), true))
                }
                Err(e) => {
                    eprintln!("Failed to check Xvfb status: {}", e);
                    eprintln!("Screenshots will be disabled");
                    Ok((None, false))
                }
            }
        }
        Err(e) => {
            eprintln!("Failed to start Xvfb: {}", e);
            eprintln!("Note: Install xvfb-run or Xvfb for screenshot support in headless environments");
            eprintln!("Continuing without screenshots...");
            Ok((None, false))
        }
    }
}

fn generate_index(widgets: &[&WidgetStates]) -> String {
    let mut content = String::new();
    content.push_str("# Waft UI GTK Widget Documentation\n\n");
    content.push_str("This directory contains documentation for all waft-ui-gtk widget types.\n\n");
    content.push_str("## Widget Types\n\n");

    for widget in widgets {
        content.push_str(&format!("### [{}]({})\n\n", widget.name, widget.name));
        content.push_str(&format!("{}\n\n", widget.description));
        content.push_str(&format!("**States:** {}\n\n", widget.states.len()));
    }

    content
}
