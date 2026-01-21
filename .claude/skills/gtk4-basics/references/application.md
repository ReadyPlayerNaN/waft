# GTK4 Application Reference

## Application

The main entry point for GTK4 applications.

### Construction

```rust
// Using builder (recommended)
let app = Application::builder()
    .application_id("org.example.MyApp")
    .flags(ApplicationFlags::FLAGS_NONE)
    .build();

// Using new()
let app = Application::new(
    Some("org.example.MyApp"),
    ApplicationFlags::FLAGS_NONE,
);
```

### Application ID Rules

- Must be reverse domain notation: `"org.example.AppName"`
- Only ASCII letters, digits, hyphens, and periods
- Must contain at least one period
- Cannot start or end with a period
- No consecutive periods
- Optional: pass `None` for development (not recommended for release)

### Lifecycle Signals

```rust
// Called once when application starts
app.connect_startup(|app| {
    // Setup that should happen once: load CSS, create actions
    println!("Application starting up");
});

// Called each time application is activated (can be multiple times)
app.connect_activate(|app| {
    // Create and show windows here
    let window = ApplicationWindow::builder()
        .application(app)
        .build();
    window.present();
});

// Called when application receives files to open
app.connect_open(|app, files, hint| {
    for file in files {
        println!("Open: {:?}", file.path());
    }
});

// Called when application is shutting down
app.connect_shutdown(|app| {
    println!("Application shutting down");
});
```

### Running the Application

```rust
fn main() -> glib::ExitCode {
    let app = Application::builder()
        .application_id("org.example.MyApp")
        .build();

    app.connect_activate(build_ui);

    // Run with command line args
    app.run()

    // Or run with specific args
    // app.run_with_args(&["arg1", "arg2"])
}
```

### ApplicationFlags

```rust
use gtk::gio::ApplicationFlags;

let app = Application::builder()
    .application_id("org.example.MyApp")
    .flags(ApplicationFlags::HANDLES_OPEN)  // Handle file opening
    .build();

// Common flags:
// FLAGS_NONE - Default behavior
// HANDLES_OPEN - Handle open signal for files
// HANDLES_COMMAND_LINE - Handle command line in primary instance
// NON_UNIQUE - Allow multiple instances
```

## ApplicationWindow

Top-level window associated with an Application.

### Construction

```rust
let window = ApplicationWindow::builder()
    .application(app)
    .title("Window Title")
    .default_width(800)
    .default_height(600)
    .child(&content_widget)
    .build();
```

### Common Properties

```rust
let window = ApplicationWindow::builder()
    .application(app)
    .title("My App")
    .default_width(800)
    .default_height(600)
    .maximized(false)
    .fullscreened(false)
    .resizable(true)
    .modal(false)
    .decorated(true)  // Window decorations
    .deletable(true)  // Close button
    .child(&content)
    .build();
```

### Window Methods

```rust
// Show window
window.present();  // Preferred - handles focus
window.show();     // Alternative

// Window state
window.maximize();
window.unmaximize();
window.fullscreen();
window.unfullscreen();
window.minimize();
window.close();

// Content
window.set_child(Some(&widget));
window.set_titlebar(Some(&header_bar));

// Sizing
window.set_default_size(800, 600);
window.set_size_request(400, 300);  // Minimum size
```

### Window Signals

```rust
// Called when close button clicked
window.connect_close_request(|window| {
    println!("Window closing");
    // Return Propagation::Stop to prevent close
    // Return Propagation::Proceed to allow close
    glib::Propagation::Proceed
});

// Window state changes
window.connect_maximized_notify(|window| {
    println!("Maximized: {}", window.is_maximized());
});

window.connect_fullscreened_notify(|window| {
    println!("Fullscreen: {}", window.is_fullscreened());
});
```

## Application with Actions

```rust
use gtk::gio::SimpleAction;

app.connect_startup(|app| {
    // Create action
    let quit_action = SimpleAction::new("quit", None);
    quit_action.connect_activate(glib::clone!(
        #[weak] app,
        move |_, _| {
            app.quit();
        }
    ));
    app.add_action(&quit_action);

    // Set keyboard shortcut
    app.set_accels_for_action("app.quit", &["<Ctrl>Q"]);
});
```

## Complete Example

```rust
use gtk4 as gtk;
use gtk::prelude::*;
use gtk::{glib, Application, ApplicationWindow, HeaderBar, Button};
use gtk::gio::SimpleAction;

fn main() -> glib::ExitCode {
    let app = Application::builder()
        .application_id("org.example.CompleteApp")
        .build();

    app.connect_startup(setup_actions);
    app.connect_activate(build_ui);

    app.run()
}

fn setup_actions(app: &Application) {
    let quit = SimpleAction::new("quit", None);
    quit.connect_activate(glib::clone!(
        #[weak] app,
        move |_, _| app.quit()
    ));
    app.add_action(&quit);
    app.set_accels_for_action("app.quit", &["<Ctrl>Q"]);
}

fn build_ui(app: &Application) {
    let header = HeaderBar::new();

    let content = Button::with_label("Hello, GTK4!");
    content.connect_clicked(|btn| btn.set_label("Clicked!"));

    let window = ApplicationWindow::builder()
        .application(app)
        .title("Complete Example")
        .default_width(400)
        .default_height(300)
        .child(&content)
        .build();

    window.set_titlebar(Some(&header));
    window.present();
}
```
