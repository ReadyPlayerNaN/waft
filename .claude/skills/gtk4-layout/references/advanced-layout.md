# GTK4 Advanced Layout Containers Reference

## Stack

Multiple pages, one visible at a time.

### Construction

```rust
use gtk::{Stack, StackTransitionType};

let stack = Stack::new();
stack.set_transition_type(StackTransitionType::SlideLeftRight);
stack.set_transition_duration(200);
```

### Adding Pages

```rust
// Add with name and title
stack.add_titled(&page1, Some("page1"), "First Page");
stack.add_titled(&page2, Some("page2"), "Second Page");

// Add with name only
stack.add_named(&page3, Some("page3"));

// Add without name (auto-generated)
stack.add_child(&page4);

// Remove page
stack.remove(&page1);
```

### Navigation

```rust
// Switch by name
stack.set_visible_child_name("page2");

// Switch by widget
stack.set_visible_child(&page2);

// Get current
let name = stack.visible_child_name();
let widget = stack.visible_child();

// Check if page exists
if let Some(page) = stack.child_by_name("page1") {
    // Use page
}
```

### StackSwitcher

Tab bar for Stack.

```rust
use gtk::StackSwitcher;

let switcher = StackSwitcher::new();
switcher.set_stack(Some(&stack));

// Layout
let container = Box::new(Orientation::Vertical, 0);
container.append(&switcher);
container.append(&stack);
```

### StackSidebar

Sidebar navigation for Stack.

```rust
use gtk::StackSidebar;

let sidebar = StackSidebar::new();
sidebar.set_stack(&stack);

// Layout with Paned
let paned = Paned::new(Orientation::Horizontal);
paned.set_start_child(Some(&sidebar));
paned.set_end_child(Some(&stack));
paned.set_position(200);  // Sidebar width
```

### Transition Types

```rust
stack.set_transition_type(StackTransitionType::None);
stack.set_transition_type(StackTransitionType::Crossfade);
stack.set_transition_type(StackTransitionType::SlideRight);
stack.set_transition_type(StackTransitionType::SlideLeft);
stack.set_transition_type(StackTransitionType::SlideUp);
stack.set_transition_type(StackTransitionType::SlideDown);
stack.set_transition_type(StackTransitionType::SlideLeftRight);
stack.set_transition_type(StackTransitionType::SlideUpDown);
stack.set_transition_type(StackTransitionType::OverUp);
stack.set_transition_type(StackTransitionType::OverDown);
stack.set_transition_type(StackTransitionType::OverLeft);
stack.set_transition_type(StackTransitionType::OverRight);
```

## Notebook

Traditional tabbed container.

```rust
use gtk::Notebook;

let notebook = Notebook::new();

// Add pages with labels
notebook.append_page(&page1, Some(&Label::new(Some("Tab 1"))));
notebook.append_page(&page2, Some(&Label::new(Some("Tab 2"))));

// Insert at position
notebook.insert_page(&page3, Some(&Label::new(Some("Tab 3"))), Some(1));

// Remove
notebook.remove_page(Some(0));
```

### Notebook Properties

```rust
// Tab position
notebook.set_tab_pos(gtk::PositionType::Top);
notebook.set_tab_pos(gtk::PositionType::Bottom);
notebook.set_tab_pos(gtk::PositionType::Left);
notebook.set_tab_pos(gtk::PositionType::Right);

// Scrollable tabs
notebook.set_scrollable(true);

// Show tabs
notebook.set_show_tabs(true);

// Show border
notebook.set_show_border(true);
```

### Notebook Navigation

```rust
// Get/set current page
let current = notebook.current_page();
notebook.set_current_page(Some(1));

// Navigate
notebook.next_page();
notebook.prev_page();

// Get page widget
if let Some(page) = notebook.nth_page(Some(0)) {
    // Use page
}
```

### Notebook Signals

```rust
notebook.connect_switch_page(|notebook, page, page_num| {
    println!("Switched to page {}", page_num);
});

notebook.connect_page_added(|notebook, child, page_num| {
    println!("Page added at {}", page_num);
});

notebook.connect_page_removed(|notebook, child, page_num| {
    println!("Page removed from {}", page_num);
});
```

## Paned

Resizable split container.

```rust
use gtk::{Paned, Orientation};

// Horizontal split
let hpaned = Paned::new(Orientation::Horizontal);
hpaned.set_start_child(Some(&left_widget));
hpaned.set_end_child(Some(&right_widget));

// Vertical split
let vpaned = Paned::new(Orientation::Vertical);
vpaned.set_start_child(Some(&top_widget));
vpaned.set_end_child(Some(&bottom_widget));
```

### Paned Properties

```rust
// Divider position in pixels
paned.set_position(200);
let pos = paned.position();

// Allow shrinking below minimum size
paned.set_shrink_start_child(true);
paned.set_shrink_end_child(true);

// Allow resize
paned.set_resize_start_child(true);
paned.set_resize_end_child(true);

// Wide handle (easier to grab)
paned.set_wide_handle(true);
```

### Nested Paned

```rust
// Three-way split
let hpaned = Paned::new(Orientation::Horizontal);
let vpaned = Paned::new(Orientation::Vertical);

hpaned.set_start_child(Some(&sidebar));
hpaned.set_end_child(Some(&vpaned));

vpaned.set_start_child(Some(&main_content));
vpaned.set_end_child(Some(&bottom_panel));
```

## ScrolledWindow

Scrollable viewport for content.

```rust
use gtk::{ScrolledWindow, PolicyType};

let scrolled = ScrolledWindow::new();
scrolled.set_child(Some(&content));
```

### Scroll Policy

```rust
// Always show scrollbars
scrolled.set_policy(PolicyType::Always, PolicyType::Always);

// Automatic (show when needed)
scrolled.set_policy(PolicyType::Automatic, PolicyType::Automatic);

// Never show horizontal, automatic vertical
scrolled.set_policy(PolicyType::Never, PolicyType::Automatic);

// Individual
scrolled.set_hscrollbar_policy(PolicyType::Never);
scrolled.set_vscrollbar_policy(PolicyType::Automatic);
```

### Size Constraints

```rust
// Minimum content size
scrolled.set_min_content_width(300);
scrolled.set_min_content_height(200);

// Maximum content size
scrolled.set_max_content_width(800);
scrolled.set_max_content_height(600);

// Propagate natural size
scrolled.set_propagate_natural_width(true);
scrolled.set_propagate_natural_height(true);
```

### Kinetic Scrolling

```rust
scrolled.set_kinetic_scrolling(true);
```

### Scroll To

```rust
// Get adjustment
let vadj = scrolled.vadjustment();
let hadj = scrolled.hadjustment();

// Scroll to position
vadj.set_value(100.0);

// Scroll to top
vadj.set_value(0.0);

// Scroll to bottom
vadj.set_value(vadj.upper() - vadj.page_size());
```

## Overlay

Layer widgets on top of each other.

```rust
use gtk::Overlay;

let overlay = Overlay::new();

// Base widget (fills the overlay)
overlay.set_child(Some(&main_content));

// Overlaid widgets
overlay.add_overlay(&floating_button);
overlay.add_overlay(&notification);
```

### Overlay Positioning

```rust
// Measure position in overlay
overlay.set_measure_overlay(&floating_button, true);

// Clip to overlay bounds
overlay.set_clip_overlay(&floating_button, true);

// Position with alignment
floating_button.set_halign(gtk::Align::End);
floating_button.set_valign(gtk::Align::End);
floating_button.set_margin_end(10);
floating_button.set_margin_bottom(10);
```

## Revealer

Animated show/hide container.

```rust
use gtk::{Revealer, RevealerTransitionType};

let revealer = Revealer::new();
revealer.set_child(Some(&content));
revealer.set_reveal_child(false);  // Start hidden
revealer.set_transition_type(RevealerTransitionType::SlideDown);
revealer.set_transition_duration(200);

// Toggle visibility
button.connect_clicked(glib::clone!(
    #[weak] revealer,
    move |_| {
        revealer.set_reveal_child(!revealer.reveals_child());
    }
));
```

### Transition Types

```rust
revealer.set_transition_type(RevealerTransitionType::None);
revealer.set_transition_type(RevealerTransitionType::Crossfade);
revealer.set_transition_type(RevealerTransitionType::SlideRight);
revealer.set_transition_type(RevealerTransitionType::SlideLeft);
revealer.set_transition_type(RevealerTransitionType::SlideUp);
revealer.set_transition_type(RevealerTransitionType::SlideDown);
revealer.set_transition_type(RevealerTransitionType::SwingRight);
revealer.set_transition_type(RevealerTransitionType::SwingLeft);
revealer.set_transition_type(RevealerTransitionType::SwingUp);
revealer.set_transition_type(RevealerTransitionType::SwingDown);
```

## Viewport

Scrollable container (usually use ScrolledWindow instead).

```rust
use gtk::Viewport;

let viewport = Viewport::new(
    Some(&gtk::Adjustment::new(0.0, 0.0, 100.0, 1.0, 10.0, 10.0)),
    Some(&gtk::Adjustment::new(0.0, 0.0, 100.0, 1.0, 10.0, 10.0)),
);
viewport.set_child(Some(&content));
```
