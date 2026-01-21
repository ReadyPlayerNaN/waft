# GTK4 Common Widgets Reference

## Button

Clickable button widget.

### Construction

```rust
// With text label
let button = Button::with_label("Click me");

// With icon
let button = Button::from_icon_name("document-open");

// With mnemonic (Alt+C activates)
let button = Button::with_mnemonic("_Click me");

// Builder pattern
let button = Button::builder()
    .label("Click")
    .has_frame(true)
    .build();
```

### Properties

```rust
button.set_label("New Label");
button.set_icon_name("icon-name");
button.set_has_frame(false);  // Flat button
button.set_child(Some(&widget));  // Custom content
button.set_can_shrink(true);  // v4.12+
```

### Signals

```rust
button.connect_clicked(|button| {
    println!("Clicked!");
});

button.connect_activate(|button| {
    // Programmatic activation
});
```

## Label

Text display widget.

### Construction

```rust
let label = Label::new(Some("Hello World"));
let label = Label::with_mnemonic("Press Alt+_H");

let label = Label::builder()
    .label("Text")
    .wrap(true)
    .justify(gtk::Justification::Center)
    .build();
```

### Text Setting

```rust
// Plain text
label.set_label("Plain text");
label.set_text("Also plain text");

// Pango markup
label.set_markup("<b>Bold</b> <i>Italic</i> <u>Underline</u>");
label.set_markup("<span foreground='red'>Red text</span>");
label.set_markup("<span size='large'>Large</span>");

// With mnemonic
label.set_text_with_mnemonic("Press _Enter");
```

### Text Styling

```rust
// Alignment
label.set_xalign(0.0);  // Left
label.set_xalign(0.5);  // Center
label.set_xalign(1.0);  // Right
label.set_yalign(0.5);  // Vertical center

// Wrapping
label.set_wrap(true);
label.set_wrap_mode(gtk::pango::WrapMode::Word);
label.set_lines(3);  // Max lines

// Ellipsis for overflow
label.set_ellipsize(gtk::pango::EllipsizeMode::End);
label.set_max_width_chars(50);
label.set_width_chars(20);  // Minimum width

// Justification
label.set_justify(gtk::Justification::Left);
label.set_justify(gtk::Justification::Center);
label.set_justify(gtk::Justification::Right);
label.set_justify(gtk::Justification::Fill);
```

### Selection

```rust
label.set_selectable(true);  // Allow text selection
label.select_region(0, 5);   // Select characters 0-5
let (start, end) = label.selection_bounds();
```

## Entry

Single-line text input.

### Construction

```rust
let entry = Entry::new();

let entry = Entry::builder()
    .placeholder_text("Enter name...")
    .max_length(100)
    .build();
```

### Text Access

```rust
// Get text (returns GString)
let text = entry.text();
let rust_string = text.to_string();

// Set text
entry.set_text("New text");

// Clear
entry.set_text("");
// Or delete all
entry.delete_text(0, -1);
```

### Properties

```rust
entry.set_placeholder_text(Some("Hint text"));
entry.set_max_length(50);
entry.set_visibility(false);  // Password mode
entry.set_editable(true);
entry.set_input_purpose(gtk::InputPurpose::Password);
entry.set_input_hints(gtk::InputHints::SPELLCHECK);
```

### Signals

```rust
// Text changed
entry.connect_changed(|entry| {
    println!("Text: {}", entry.text());
});

// Enter pressed
entry.connect_activate(|entry| {
    println!("Submitted: {}", entry.text());
});

// Insert validation
entry.connect_insert_text(|entry, text, position| {
    // Can modify text before insertion
});
```

### Icons

```rust
// Set icons
entry.set_primary_icon_name(Some("edit-find"));
entry.set_secondary_icon_name(Some("edit-clear"));

// Handle icon clicks
entry.connect_icon_press(|entry, position| {
    match position {
        gtk::EntryIconPosition::Primary => println!("Search clicked"),
        gtk::EntryIconPosition::Secondary => entry.set_text(""),
        _ => {}
    }
});
```

## PasswordEntry

Specialized entry for passwords.

```rust
use gtk::PasswordEntry;

let password = PasswordEntry::new();
password.set_show_peek_icon(true);  // Show/hide toggle

password.connect_activate(|entry| {
    let pw = entry.text();
});
```

## Switch

On/off toggle switch.

```rust
use gtk::Switch;

let switch = Switch::new();
switch.set_active(true);

switch.connect_state_set(|switch, state| {
    println!("Switch: {}", state);
    glib::Propagation::Proceed
});
```

## Scale

Slider for numeric values.

```rust
use gtk::Scale;

let scale = Scale::with_range(gtk::Orientation::Horizontal, 0.0, 100.0, 1.0);
scale.set_value(50.0);
scale.set_draw_value(true);  // Show current value

scale.connect_value_changed(|scale| {
    println!("Value: {}", scale.value());
});

// Add marks
scale.add_mark(0.0, gtk::PositionType::Bottom, Some("Min"));
scale.add_mark(50.0, gtk::PositionType::Bottom, Some("Mid"));
scale.add_mark(100.0, gtk::PositionType::Bottom, Some("Max"));
```

## SpinButton

Numeric input with increment/decrement.

```rust
use gtk::SpinButton;

let spin = SpinButton::with_range(0.0, 100.0, 1.0);
spin.set_value(50.0);
spin.set_digits(0);  // Decimal places

spin.connect_value_changed(|spin| {
    println!("Value: {}", spin.value_as_int());
});
```

## Image

Display images.

```rust
use gtk::Image;

// From icon name
let icon = Image::from_icon_name("document-open");

// From file
let image = Image::from_file("/path/to/image.png");

// From resource
let image = Image::from_resource("/org/example/image.png");

// Set size
image.set_pixel_size(48);
```

## Separator

Visual divider.

```rust
use gtk::Separator;

let hsep = Separator::new(gtk::Orientation::Horizontal);
let vsep = Separator::new(gtk::Orientation::Vertical);
```

## ProgressBar

Progress indication.

```rust
use gtk::ProgressBar;

let progress = ProgressBar::new();
progress.set_fraction(0.5);  // 50%
progress.set_text(Some("50%"));
progress.set_show_text(true);

// Indeterminate mode
progress.pulse();

// Update in loop
glib::timeout_add_local(Duration::from_millis(100), move || {
    progress.pulse();
    glib::ControlFlow::Continue
});
```

## Spinner

Loading indicator.

```rust
use gtk::Spinner;

let spinner = Spinner::new();
spinner.start();
// Later...
spinner.stop();
```
