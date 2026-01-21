---
name: gtk4-basics
description: |
  CRITICAL: Use for GTK4 Rust application setup questions. Triggers on:
  gtk4, gtk-rs, Application, ApplicationWindow, gtk4::prelude,
  connect_activate, app.run(), GtkApplication, gtk4 hello world,
  GTK4 应用, gtk-rs 入门, GTK 窗口, Rust GUI
---

# GTK4 Basics Skill

> **Version:** gtk4 0.10.3 | **Last Updated:** 2025-01-21
>
> Check for updates: https://crates.io/crates/gtk4

You are an expert at the Rust `gtk4` crate for building GTK4 GUI applications. Help users by:
- **Writing code**: Generate GTK4 Rust code following gtk-rs patterns
- **Answering questions**: Explain Application lifecycle, signals, and threading

## Documentation

Refer to the local files for detailed documentation:
- `./references/application.md` - Application and ApplicationWindow API
- `./references/signals.md` - Signal connections and callbacks

## IMPORTANT: Documentation Completeness Check

**Before answering questions, Claude MUST:**

1. Read the relevant reference file(s) listed above
2. If file read fails or file is empty:
   - Inform user: "本地文档不完整，建议运行 `/sync-crate-skills gtk4 --force` 更新文档"
   - Still answer based on SKILL.md patterns + built-in knowledge
3. If reference file exists, incorporate its content into the answer

## Key Patterns

### 1. Minimal GTK4 Application

```rust
use gtk4 as gtk;
use gtk::prelude::*;
use gtk::{glib, Application, ApplicationWindow};

fn main() -> glib::ExitCode {
    let app = Application::builder()
        .application_id("org.example.MyApp")
        .build();

    app.connect_activate(|app| {
        let window = ApplicationWindow::builder()
            .application(app)
            .title("My Application")
            .default_width(400)
            .default_height(300)
            .build();

        window.present();
    });

    app.run()
}
```

### 2. Application with Content

```rust
use gtk4 as gtk;
use gtk::prelude::*;
use gtk::{glib, Application, ApplicationWindow, Button, Box, Orientation};

fn main() -> glib::ExitCode {
    let app = Application::builder()
        .application_id("org.example.ButtonApp")
        .build();

    app.connect_activate(build_ui);
    app.run()
}

fn build_ui(app: &Application) {
    let button = Button::with_label("Click me!");
    button.connect_clicked(|button| {
        button.set_label("Clicked!");
    });

    let container = Box::new(Orientation::Vertical, 10);
    container.set_margin_all(10);
    container.append(&button);

    let window = ApplicationWindow::builder()
        .application(app)
        .title("Button Example")
        .child(&container)
        .build();

    window.present();
}
```

### 3. Signal Connections with Clone

```rust
use gtk4 as gtk;
use gtk::prelude::*;
use gtk::{Button, Label};

// Clone widgets for use in closures
let label = Label::new(Some("Count: 0"));
let button = Button::with_label("Increment");

let label_clone = label.clone();
button.connect_clicked(move |_| {
    // Use cloned reference inside closure
    label_clone.set_label("Clicked!");
});
```

## API Reference Table

| Type | Description | Usage |
|------|-------------|-------|
| `Application` | Main application object | `Application::builder().application_id(id).build()` |
| `ApplicationWindow` | Top-level window | `ApplicationWindow::builder().application(app).build()` |
| `glib::ExitCode` | Return type for main | `fn main() -> glib::ExitCode` |
| `prelude::*` | Common traits | `use gtk::prelude::*;` |

## Deprecated Patterns (Don't Use)

| Deprecated | Correct | Notes |
|------------|---------|-------|
| `gtk::init()` manually | Use `Application` | Application handles init |
| `gtk::main()` | `app.run()` | Application manages main loop |
| Raw widget construction | Builder pattern | `Widget::builder().prop(val).build()` |

## When Writing Code

1. Always use `Application` - it handles `gtk::init()` automatically
2. Use builder pattern for constructing widgets: `Widget::builder().build()`
3. Clone widgets before moving into closures with `widget.clone()`
4. Application ID must be reverse domain notation: `"org.example.AppName"`
5. Call `window.present()` to show windows (not `show()`)
6. Return `glib::ExitCode` from main, not `()`

## When Answering Questions

1. GTK4 is NOT thread-safe - all UI must be on main thread
2. Use `glib::idle_add()` or `glib::spawn_future_local()` for async
3. Widgets are reference-counted - cloning is cheap
4. `connect_*` methods take closures for signal handling
5. The prelude provides extension traits - always import it
