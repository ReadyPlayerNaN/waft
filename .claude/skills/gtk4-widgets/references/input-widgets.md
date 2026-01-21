# GTK4 Input and Selection Widgets Reference

## CheckButton

Checkbox widget.

```rust
use gtk::CheckButton;

// Basic
let check = CheckButton::new();
let check = CheckButton::with_label("Enable");
let check = CheckButton::with_mnemonic("_Enable");

// Builder
let check = CheckButton::builder()
    .label("Enable feature")
    .active(true)
    .build();

// State
check.set_active(true);
let is_checked = check.is_active();

// Inconsistent state (tristate)
check.set_inconsistent(true);

// Signal
check.connect_toggled(|check| {
    println!("Active: {}", check.is_active());
});
```

### Radio Buttons with CheckButton

```rust
let radio1 = CheckButton::with_label("Option 1");
let radio2 = CheckButton::with_label("Option 2");
let radio3 = CheckButton::with_label("Option 3");

// Group them - only one can be active
radio2.set_group(Some(&radio1));
radio3.set_group(Some(&radio1));

// Set default
radio1.set_active(true);

// Check which is selected
radio1.connect_toggled(|radio| {
    if radio.is_active() {
        println!("Option 1 selected");
    }
});
```

## ToggleButton

Button that stays pressed.

```rust
use gtk::ToggleButton;

let toggle = ToggleButton::with_label("Toggle");

toggle.connect_toggled(|button| {
    if button.is_active() {
        button.set_label("ON");
    } else {
        button.set_label("OFF");
    }
});
```

## DropDown (Recommended over ComboBox)

Modern dropdown selection.

```rust
use gtk::DropDown;

// From string array
let options = ["Option 1", "Option 2", "Option 3"];
let dropdown = DropDown::from_strings(&options);

// Set selection
dropdown.set_selected(0);

// Get selection
dropdown.connect_selected_notify(|dropdown| {
    let index = dropdown.selected();
    println!("Selected index: {}", index);

    // Get selected item
    if let Some(item) = dropdown.selected_item() {
        if let Some(string_obj) = item.downcast_ref::<gtk::StringObject>() {
            println!("Selected: {}", string_obj.string());
        }
    }
});
```

### DropDown with Custom Model

```rust
use gtk::{DropDown, StringList, SignalListItemFactory, ListItem, Label};

let model = StringList::new(&["Red", "Green", "Blue"]);
let dropdown = DropDown::new(Some(model), None::<gtk::Expression>);

// Custom display
let factory = SignalListItemFactory::new();
factory.connect_setup(|_, item| {
    let label = Label::new(None);
    item.downcast_ref::<ListItem>().unwrap().set_child(Some(&label));
});
factory.connect_bind(|_, item| {
    let item = item.downcast_ref::<ListItem>().unwrap();
    let string_obj = item.item().and_downcast::<gtk::StringObject>().unwrap();
    let label = item.child().and_downcast::<Label>().unwrap();
    label.set_label(&string_obj.string());
});
dropdown.set_factory(Some(&factory));
```

## ComboBoxText (Deprecated, use DropDown)

```rust
use gtk::ComboBoxText;

let combo = ComboBoxText::new();
combo.append_text("Option 1");
combo.append_text("Option 2");
combo.append(Some("id1"), "Option with ID");

combo.set_active(Some(0));

combo.connect_changed(|combo| {
    if let Some(text) = combo.active_text() {
        println!("Selected: {}", text);
    }
});
```

## TextView

Multi-line text editing.

```rust
use gtk::{TextView, TextBuffer};

// Basic
let textview = TextView::new();

// With buffer
let buffer = TextBuffer::new(None);
buffer.set_text("Initial text");
let textview = TextView::with_buffer(&buffer);

// Builder
let textview = TextView::builder()
    .editable(true)
    .wrap_mode(gtk::WrapMode::Word)
    .monospace(true)
    .build();
```

### Text Operations

```rust
let buffer = textview.buffer();

// Set all text
buffer.set_text("New content");

// Get all text
let start = buffer.start_iter();
let end = buffer.end_iter();
let text = buffer.text(&start, &end, true);

// Insert at cursor
buffer.insert_at_cursor("Inserted text");

// Get/set cursor position
let mark = buffer.get_insert();
let iter = buffer.iter_at_mark(&mark);
let offset = iter.offset();

// Selection
let (has_selection, start, end) = buffer.selection_bounds();
if has_selection {
    let selected = buffer.text(&start, &end, true);
}
```

### Text Signals

```rust
let buffer = textview.buffer();

buffer.connect_changed(|buffer| {
    println!("Text changed");
});

buffer.connect_insert_text(|buffer, location, text| {
    println!("Inserting: {} at {}", text, location.offset());
});

buffer.connect_delete_range(|buffer, start, end| {
    println!("Deleting from {} to {}", start.offset(), end.offset());
});
```

### Text Tags (Styling)

```rust
let buffer = textview.buffer();
let tag_table = buffer.tag_table();

// Create tag
let bold_tag = gtk::TextTag::builder()
    .name("bold")
    .weight(700)
    .build();
tag_table.add(&bold_tag);

// Apply tag
let start = buffer.iter_at_offset(0);
let end = buffer.iter_at_offset(5);
buffer.apply_tag_by_name("bold", &start, &end);

// Remove tag
buffer.remove_tag_by_name("bold", &start, &end);
```

## ListBox

Vertical list of selectable rows.

```rust
use gtk::{ListBox, ListBoxRow, Label};

let listbox = ListBox::new();
listbox.set_selection_mode(gtk::SelectionMode::Single);

// Add rows
for i in 0..10 {
    let label = Label::new(Some(&format!("Item {}", i)));
    listbox.append(&label);
}

// Selection handling
listbox.connect_row_selected(|_, row| {
    if let Some(row) = row {
        println!("Selected row: {}", row.index());
    }
});

// Row activation (double-click or Enter)
listbox.connect_row_activated(|_, row| {
    println!("Activated row: {}", row.index());
});
```

### ListBox with Model

```rust
use gtk::{ListBox, SignalListItemFactory, ListItem, Label};
use gtk::gio::ListStore;

// Create model
let model = ListStore::new::<gtk::StringObject>();
model.append(&gtk::StringObject::new("Item 1"));
model.append(&gtk::StringObject::new("Item 2"));

// Bind to ListBox
listbox.bind_model(Some(&model), |item| {
    let string_obj = item.downcast_ref::<gtk::StringObject>().unwrap();
    let label = Label::new(Some(&string_obj.string()));
    label.upcast()
});
```

## FlowBox

Grid that reflows based on available space.

```rust
use gtk::FlowBox;

let flowbox = FlowBox::new();
flowbox.set_selection_mode(gtk::SelectionMode::Multiple);
flowbox.set_max_children_per_line(5);
flowbox.set_min_children_per_line(2);

// Add children
for i in 0..20 {
    let button = Button::with_label(&format!("Item {}", i));
    flowbox.append(&button);
}

// Selection
flowbox.connect_selected_children_changed(|flowbox| {
    let selected = flowbox.selected_children();
    println!("Selected {} items", selected.len());
});
```

## SearchEntry

Entry with search-specific features.

```rust
use gtk::SearchEntry;

let search = SearchEntry::new();

// Delayed search (after typing stops)
search.connect_search_changed(|entry| {
    let query = entry.text();
    println!("Searching: {}", query);
});

// Immediate search on Enter
search.connect_activate(|entry| {
    println!("Search: {}", entry.text());
});

// Stop search (Escape key)
search.connect_stop_search(|_| {
    println!("Search cancelled");
});
```

## ColorButton

Color picker button.

```rust
use gtk::ColorButton;
use gtk::gdk::RGBA;

let color_btn = ColorButton::new();
color_btn.set_rgba(&RGBA::new(1.0, 0.0, 0.0, 1.0));  // Red

color_btn.connect_color_set(|btn| {
    let color = btn.rgba();
    println!("Color: r={}, g={}, b={}", color.red(), color.green(), color.blue());
});
```

## FontButton

Font picker button.

```rust
use gtk::FontButton;

let font_btn = FontButton::new();
font_btn.set_font("Sans 12");

font_btn.connect_font_set(|btn| {
    if let Some(font) = btn.font() {
        println!("Font: {}", font);
    }
});
```
