use anyhow::{Context, Result};
use gtk::prelude::*;
use std::path::Path;
use std::rc::Rc;
use std::cell::RefCell;
use std::process::Command;
use waft_ui_gtk::types::Widget;
use waft_ui_gtk::renderer::WidgetRenderer;
use waft_core::menu_state::create_menu_store;

// Global window that persists for the lifetime of the program
thread_local! {
    static GLOBAL_WINDOW: RefCell<Option<gtk::Window>> = RefCell::new(None);
}

fn get_global_window() -> gtk::Window {
    GLOBAL_WINDOW.with(|cell| {
        let mut cell_mut = cell.borrow_mut();
        if let Some(ref window) = *cell_mut {
            return window.clone();
        }

        let window = gtk::Window::builder()
            .decorated(false)
            .default_width(800)
            .default_height(600)
            .title("Widget Documentation")
            .build();

        window.present();

        // Process events to ensure the window is shown
        let context = gtk::glib::MainContext::default();
        for _ in 0..10 {
            context.iteration(false);
        }

        *cell_mut = Some(window.clone());
        window
    })
}

/// Get the X11 window ID for a GTK window
fn get_x11_window_id(window: &gtk::Window) -> Result<String> {
    let surface = window.surface()
        .context("Failed to get window surface")?;

    // Get the X11 window ID from GDK
    #[cfg(target_os = "linux")]
    {
        use gtk::gdk::prelude::*;

        if let Some(x11_surface) = surface.downcast_ref::<gdk::X11Surface>() {
            let xid = x11_surface.xid();
            return Ok(format!("0x{:x}", xid));
        }
    }

    anyhow::bail!("Could not get X11 window ID")
}

/// Capture a screenshot of a widget and save it as PNG.
///
/// Uses ImageMagick's import command to capture the X11 window.
pub fn capture_widget_screenshot(widget: &Widget, output_path: &Path) -> Result<()> {
    // Create widget renderer
    let menu_store = Rc::new(create_menu_store());
    let action_callback = Rc::new(|_widget_id: String, _action| {});
    let renderer = WidgetRenderer::new(menu_store, action_callback);

    // Render the widget
    let gtk_widget = renderer.render(widget, "doc_widget");

    // Get or create the global window
    let window = get_global_window();

    // Set the widget as the window's child
    window.set_child(Some(&gtk_widget));

    // Ensure visibility
    gtk_widget.set_visible(true);
    window.set_visible(true);

    // Wait for the widget to be mapped
    let mut iterations = 0;
    while !gtk_widget.is_mapped() && iterations < 100 {
        gtk::glib::MainContext::default().iteration(true);
        iterations += 1;
    }

    if !gtk_widget.is_mapped() {
        anyhow::bail!("Widget failed to map after {} iterations", iterations);
    }

    // Process more events to ensure complete layout
    for _ in 0..20 {
        gtk::glib::MainContext::default().iteration(false);
    }

    // Get allocated size
    let width = gtk_widget.allocated_width().max(100);
    let height = gtk_widget.allocated_height().max(50);

    // Resize window to fit widget
    window.set_default_size(width, height);

    // Process more events for resize
    for _ in 0..10 {
        gtk::glib::MainContext::default().iteration(false);
    }

    // Get X11 window ID
    let window_id = get_x11_window_id(&window)?;

    // Use ImageMagick's import to capture the window
    let output = Command::new("import")
        .args(&[
            "-window", &window_id,
            "-crop", &format!("{}x{}+0+0", width, height),
            "+repage",
            output_path.to_str().context("Invalid output path")?,
        ])
        .output()
        .context("Failed to run import command")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("import command failed: {}", stderr);
    }

    println!("      Saved {}x{} screenshot to {:?}", width, height, output_path);

    Ok(())
}
