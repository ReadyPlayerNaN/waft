# GTK4 Layout Containers Reference

## Box

Linear container for arranging widgets horizontally or vertically.

### Construction

```rust
use gtk::{Box, Orientation};

// Vertical with 10px spacing
let vbox = Box::new(Orientation::Vertical, 10);

// Horizontal with 5px spacing
let hbox = Box::new(Orientation::Horizontal, 5);

// Builder pattern
let container = Box::builder()
    .orientation(Orientation::Vertical)
    .spacing(10)
    .margin_start(10)
    .margin_end(10)
    .margin_top(10)
    .margin_bottom(10)
    .build();
```

### Adding Children

```rust
// Add to end
container.append(&widget1);
container.append(&widget2);

// Add to start
container.prepend(&widget3);

// Insert after specific widget
container.insert_child_after(&new_widget, Some(&existing_widget));

// Remove
container.remove(&widget);
```

### Spacing and Margins

```rust
// Space between children
container.set_spacing(10);

// Margin around container
container.set_margin_all(10);
// Or individual
container.set_margin_start(10);
container.set_margin_end(10);
container.set_margin_top(10);
container.set_margin_bottom(10);

// Equal size children
container.set_homogeneous(true);
```

### Widget Expansion

```rust
// Make widget expand horizontally
widget.set_hexpand(true);

// Make widget expand vertically
widget.set_vexpand(true);

// Alignment within allocated space
widget.set_halign(gtk::Align::Start);   // Left
widget.set_halign(gtk::Align::Center);  // Center
widget.set_halign(gtk::Align::End);     // Right
widget.set_halign(gtk::Align::Fill);    // Fill space

widget.set_valign(gtk::Align::Start);   // Top
widget.set_valign(gtk::Align::Center);  // Middle
widget.set_valign(gtk::Align::End);     // Bottom
widget.set_valign(gtk::Align::Fill);    // Fill space
```

## Grid

Two-dimensional layout for forms and tables.

### Construction

```rust
use gtk::Grid;

let grid = Grid::new();

let grid = Grid::builder()
    .row_spacing(10)
    .column_spacing(10)
    .margin_all(10)
    .build();
```

### Adding Children

```rust
// attach(widget, column, row, column_span, row_span)
grid.attach(&label, 0, 0, 1, 1);   // Column 0, Row 0
grid.attach(&entry, 1, 0, 1, 1);   // Column 1, Row 0
grid.attach(&button, 0, 1, 2, 1);  // Span 2 columns

// Add next to existing widget
grid.attach_next_to(
    &new_widget,
    Some(&existing_widget),
    gtk::PositionType::Right,
    1, 1
);

// Remove
grid.remove(&widget);
```

### Spacing

```rust
grid.set_row_spacing(10);
grid.set_column_spacing(10);

// Uniform rows/columns
grid.set_row_homogeneous(true);
grid.set_column_homogeneous(true);
```

### Row/Column Properties

```rust
// Set baseline alignment for row
grid.set_row_baseline_position(0, gtk::BaselinePosition::Center);

// Query grid
grid.query_child(&widget);  // Returns column, row, width, height
```

### Form Example

```rust
let grid = Grid::builder()
    .row_spacing(10)
    .column_spacing(10)
    .margin_all(10)
    .build();

let name_label = Label::new(Some("Name:"));
name_label.set_halign(gtk::Align::End);
let name_entry = Entry::new();
name_entry.set_hexpand(true);

let email_label = Label::new(Some("Email:"));
email_label.set_halign(gtk::Align::End);
let email_entry = Entry::new();
email_entry.set_hexpand(true);

let submit = Button::with_label("Submit");

grid.attach(&name_label, 0, 0, 1, 1);
grid.attach(&name_entry, 1, 0, 1, 1);
grid.attach(&email_label, 0, 1, 1, 1);
grid.attach(&email_entry, 1, 1, 1, 1);
grid.attach(&submit, 0, 2, 2, 1);  // Span both columns
```

## Frame

Container with optional label and border.

```rust
use gtk::Frame;

let frame = Frame::new(Some("Settings"));
frame.set_child(Some(&content_widget));

// Without label
let frame = Frame::new(None);
frame.set_child(Some(&content_widget));

// Label widget
let label = Label::new(Some("Custom Label"));
label.set_markup("<b>Bold Label</b>");
frame.set_label_widget(Some(&label));

// Alignment of label
frame.set_label_align(0.0);  // Left
frame.set_label_align(0.5);  // Center
frame.set_label_align(1.0);  // Right
```

## CenterBox

Three-slot horizontal or vertical layout.

```rust
use gtk::CenterBox;

let center_box = CenterBox::new();
center_box.set_start_widget(Some(&left_widget));
center_box.set_center_widget(Some(&center_widget));
center_box.set_end_widget(Some(&right_widget));

// Orientation
center_box.set_orientation(Orientation::Horizontal);
```

## HeaderBar

Modern title bar with widgets.

```rust
use gtk::HeaderBar;

let header = HeaderBar::new();

// Add widgets
header.pack_start(&menu_button);
header.pack_end(&search_button);

// Custom title widget
header.set_title_widget(Some(&custom_title));

// Show title buttons (close, minimize, maximize)
header.set_show_title_buttons(true);

// Set as window titlebar
window.set_titlebar(Some(&header));
```

## ActionBar

Bottom toolbar for actions.

```rust
use gtk::ActionBar;

let action_bar = ActionBar::new();
action_bar.pack_start(&button1);
action_bar.pack_end(&button2);
action_bar.set_center_widget(Some(&center_widget));

// Visibility
action_bar.set_revealed(true);  // Show
action_bar.set_revealed(false); // Hide with animation
```

## Expander

Collapsible container.

```rust
use gtk::Expander;

let expander = Expander::new(Some("Details"));
expander.set_child(Some(&details_widget));
expander.set_expanded(false);  // Start collapsed

expander.connect_expanded_notify(|expander| {
    println!("Expanded: {}", expander.is_expanded());
});
```

## AspectFrame

Container maintaining aspect ratio.

```rust
use gtk::AspectFrame;

let frame = AspectFrame::new(
    0.5,   // xalign (0.0 = left, 1.0 = right)
    0.5,   // yalign (0.0 = top, 1.0 = bottom)
    16.0 / 9.0,  // ratio (width / height)
    false  // obey_child - use child's aspect if true
);
frame.set_child(Some(&video_widget));
```

## Fixed

Absolute positioning (use sparingly).

```rust
use gtk::Fixed;

let fixed = Fixed::new();
fixed.put(&widget1, 10.0, 20.0);  // x=10, y=20
fixed.put(&widget2, 100.0, 50.0);

// Move widget
fixed.move_(&widget1, 30.0, 40.0);

// Remove
fixed.remove(&widget1);
```
