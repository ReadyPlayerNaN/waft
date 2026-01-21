# GTK4 Signals and Callbacks Reference

## Signal Connection Basics

GTK4 widgets emit signals for user interactions and state changes.

### Basic Pattern

```rust
widget.connect_signal_name(|widget| {
    // Handle signal
});
```

### Common Widget Signals

```rust
// Button
button.connect_clicked(|button| {
    println!("Button clicked!");
});

// Entry (text input)
entry.connect_changed(|entry| {
    let text = entry.text();
    println!("Text: {}", text);
});

entry.connect_activate(|entry| {
    // User pressed Enter
    println!("Submitted: {}", entry.text());
});

// Window
window.connect_close_request(|window| {
    println!("Close requested");
    glib::Propagation::Proceed  // Allow close
});

// CheckButton
check.connect_toggled(|check| {
    println!("Active: {}", check.is_active());
});

// Scale/SpinButton
scale.connect_value_changed(|scale| {
    println!("Value: {}", scale.value());
});
```

## Cloning Widgets for Closures

Widgets are reference-counted. Clone them to use in closures.

### Manual Clone

```rust
let label = Label::new(Some("Initial"));
let button = Button::with_label("Update");

let label_clone = label.clone();
button.connect_clicked(move |_| {
    label_clone.set_label("Updated!");
});
```

### Using glib::clone! Macro (Recommended)

```rust
use gtk::glib;

let label = Label::new(Some("Initial"));
let button = Button::with_label("Update");

button.connect_clicked(glib::clone!(
    #[weak] label,
    move |_| {
        label.set_label("Updated!");
    }
));
```

### Strong vs Weak References

```rust
// Strong reference - keeps widget alive
button.connect_clicked(glib::clone!(
    #[strong] label,
    move |_| {
        label.set_label("Updated");
    }
));

// Weak reference - allows widget to be dropped
button.connect_clicked(glib::clone!(
    #[weak] label,
    move |_| {
        label.set_label("Updated");
    }
));

// Weak with default action if widget is gone
button.connect_clicked(glib::clone!(
    #[weak] label,
    #[weak_allow_none] optional_widget,
    move |_| {
        label.set_label("Updated");
        if let Some(w) = optional_widget {
            w.show();
        }
    }
));
```

## Property Notifications

Monitor property changes with `connect_*_notify`:

```rust
// Watch for label changes
label.connect_label_notify(|label| {
    println!("Label changed to: {}", label.label());
});

// Watch for visibility changes
widget.connect_visible_notify(|widget| {
    println!("Visible: {}", widget.is_visible());
});

// Watch for sensitivity changes
widget.connect_sensitive_notify(|widget| {
    println!("Sensitive: {}", widget.is_sensitive());
});
```

## Signal Handler IDs

Store handler IDs to disconnect later:

```rust
let handler_id = button.connect_clicked(|_| {
    println!("Clicked!");
});

// Later, disconnect the handler
button.disconnect(handler_id);

// Or block/unblock temporarily
button.block_signal(&handler_id);
button.unblock_signal(&handler_id);
```

## Async Operations

GTK4 is single-threaded. Use these for async work:

### glib::spawn_future_local

```rust
use gtk::glib;

button.connect_clicked(|_| {
    glib::spawn_future_local(async {
        // Async operation that stays on main thread
        let result = some_async_operation().await;
        // Update UI directly
        label.set_label(&result);
    });
});
```

### glib::idle_add_local

```rust
use gtk::glib;

// Run on next idle
glib::idle_add_local(|| {
    // Do something on main thread
    glib::ControlFlow::Break  // Run once
});

// Run repeatedly
glib::idle_add_local(|| {
    // Check condition
    if should_continue {
        glib::ControlFlow::Continue
    } else {
        glib::ControlFlow::Break
    }
});
```

### glib::timeout_add_local

```rust
use gtk::glib;
use std::time::Duration;

// Run after delay
glib::timeout_add_local(Duration::from_secs(1), || {
    println!("1 second passed");
    glib::ControlFlow::Break
});

// Run repeatedly
glib::timeout_add_local(Duration::from_millis(100), || {
    // Update every 100ms
    glib::ControlFlow::Continue
});
```

### Cross-Thread Communication

```rust
use gtk::glib;
use std::thread;

let (sender, receiver) = glib::MainContext::channel(glib::Priority::DEFAULT);

// Spawn background thread
thread::spawn(move || {
    // Heavy computation
    let result = expensive_computation();
    sender.send(result).unwrap();
});

// Receive on main thread
receiver.attach(None, move |result| {
    // Update UI with result
    label.set_label(&result);
    glib::ControlFlow::Continue
});
```

## Event Controllers

Modern GTK4 input handling:

```rust
use gtk::{EventControllerKey, EventControllerMotion, GestureClick};

// Keyboard input
let key_controller = EventControllerKey::new();
key_controller.connect_key_pressed(|_, keyval, keycode, state| {
    println!("Key pressed: {:?}", keyval);
    glib::Propagation::Proceed
});
widget.add_controller(key_controller);

// Mouse clicks
let click_controller = GestureClick::new();
click_controller.connect_pressed(|gesture, n_press, x, y| {
    println!("Clicked at ({}, {}), {} times", x, y, n_press);
});
widget.add_controller(click_controller);

// Mouse motion
let motion_controller = EventControllerMotion::new();
motion_controller.connect_motion(|_, x, y| {
    println!("Mouse at ({}, {})", x, y);
});
widget.add_controller(motion_controller);
```

## Custom Signals in Subclasses

```rust
use gtk::glib;
use gtk::subclass::prelude::*;

#[derive(Default)]
pub struct MyWidgetPrivate {}

#[glib::object_subclass]
impl ObjectSubclass for MyWidgetPrivate {
    const NAME: &'static str = "MyWidget";
    type Type = super::MyWidget;
    type ParentType = gtk::Widget;
}

impl ObjectImpl for MyWidgetPrivate {
    fn signals() -> &'static [glib::subclass::Signal] {
        static SIGNALS: OnceLock<Vec<glib::subclass::Signal>> = OnceLock::new();
        SIGNALS.get_or_init(|| {
            vec![
                glib::subclass::Signal::builder("my-signal")
                    .param_types([String::static_type()])
                    .build(),
            ]
        })
    }
}
```
