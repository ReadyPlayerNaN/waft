---
name: gtk4-layout
description: |
  CRITICAL: Use for GTK4 layout and container questions. Triggers on:
  gtk4 Box, Grid, Paned, Stack, Notebook, ScrolledWindow,
  gtk4 layout, container, append, prepend, Orientation,
  gtk4 布局, 容器, 网格布局, 垂直布局, 水平布局
---

# GTK4 Layout Skill

> **Version:** gtk4 0.10.3 | **Last Updated:** 2025-01-21
>
> Check for updates: https://crates.io/crates/gtk4

You are an expert at the Rust `gtk4` crate layout containers. Help users by:
- **Writing code**: Generate layout container code and widget arrangement
- **Answering questions**: Explain container types, spacing, and alignment

## Documentation

Refer to the local files for detailed documentation:
- `./references/containers.md` - Box, Grid, and other layout containers
- `./references/advanced-layout.md` - Stack, Notebook, Paned, and ScrolledWindow

## IMPORTANT: Documentation Completeness Check

**Before answering questions, Claude MUST:**

1. Read the relevant reference file(s) listed above
2. If file read fails or file is empty:
   - Inform user: "本地文档不完整，建议运行 `/sync-crate-skills gtk4 --force` 更新文档"
   - Still answer based on SKILL.md patterns + built-in knowledge
3. If reference file exists, incorporate its content into the answer

## Key Patterns

### 1. Box Layout (Vertical/Horizontal)

```rust
use gtk4 as gtk;
use gtk::prelude::*;
use gtk::{Box, Orientation, Button, Label};

// Vertical box
let vbox = Box::new(Orientation::Vertical, 10);  // 10px spacing
vbox.set_margin_all(10);

vbox.append(&Label::new(Some("Header")));
vbox.append(&Button::with_label("Button 1"));
vbox.append(&Button::with_label("Button 2"));

// Horizontal box
let hbox = Box::new(Orientation::Horizontal, 5);
hbox.append(&Button::with_label("Left"));
hbox.append(&Button::with_label("Right"));
```

### 2. Grid Layout

```rust
use gtk4 as gtk;
use gtk::prelude::*;
use gtk::{Grid, Label, Entry};

let grid = Grid::new();
grid.set_row_spacing(10);
grid.set_column_spacing(10);

// attach(widget, column, row, width, height)
grid.attach(&Label::new(Some("Name:")), 0, 0, 1, 1);
grid.attach(&Entry::new(), 1, 0, 1, 1);
grid.attach(&Label::new(Some("Email:")), 0, 1, 1, 1);
grid.attach(&Entry::new(), 1, 1, 1, 1);

// Span multiple columns
grid.attach(&Button::with_label("Submit"), 0, 2, 2, 1);
```

### 3. Stack with StackSwitcher

```rust
use gtk4 as gtk;
use gtk::prelude::*;
use gtk::{Box, Orientation, Stack, StackSwitcher, Label};

let stack = Stack::new();
stack.add_titled(&Label::new(Some("Page 1 content")), Some("page1"), "Page 1");
stack.add_titled(&Label::new(Some("Page 2 content")), Some("page2"), "Page 2");

let switcher = StackSwitcher::new();
switcher.set_stack(Some(&stack));

let container = Box::new(Orientation::Vertical, 0);
container.append(&switcher);
container.append(&stack);
```

### 4. ScrolledWindow

```rust
use gtk4 as gtk;
use gtk::prelude::*;
use gtk::{ScrolledWindow, TextView};

let textview = TextView::new();
textview.set_wrap_mode(gtk::WrapMode::Word);

let scrolled = ScrolledWindow::new();
scrolled.set_child(Some(&textview));
scrolled.set_min_content_height(200);
scrolled.set_min_content_width(300);
```

## API Reference Table

| Container | Constructor | Child Method | Use Case |
|-----------|-------------|--------------|----------|
| `Box` | `new(Orientation, spacing)` | `append()`, `prepend()` | Linear layout |
| `Grid` | `new()` | `attach(w, col, row, w, h)` | Form layout |
| `Stack` | `new()` | `add_titled()` | Tabbed/paged |
| `Notebook` | `new()` | `append_page()` | Traditional tabs |
| `Paned` | `new(Orientation)` | `set_start_child()` | Resizable split |
| `ScrolledWindow` | `new()` | `set_child()` | Scrollable content |
| `Overlay` | `new()` | `set_child()`, `add_overlay()` | Layered widgets |
| `Frame` | `new(label)` | `set_child()` | Bordered group |

## Deprecated Patterns (Don't Use)

| Deprecated | Correct | Notes |
|------------|---------|-------|
| `pack_start()`/`pack_end()` | `append()`/`prepend()` | GTK3 methods removed |
| `add()` on containers | `append()` or `set_child()` | Use specific methods |
| Manual expand/fill | `set_hexpand()`/`set_vexpand()` | Widget properties |

## When Writing Code

1. Use `Box` for simple linear layouts (vertical or horizontal)
2. Use `Grid` for form-like layouts with rows/columns
3. Set `set_margin_all()` for consistent padding
4. Use `set_hexpand(true)` / `set_vexpand(true)` for expanding widgets
5. Always wrap scrollable content in `ScrolledWindow`
6. Use `Stack` for multi-page UIs, `Notebook` for visible tabs

## When Answering Questions

1. `Box` replaces GTK3's HBox/VBox with `Orientation` parameter
2. `append()` adds to end, `prepend()` adds to start
3. Grid columns/rows are 0-indexed
4. `set_homogeneous(true)` makes all children equal size
5. Spacing is between children, margin is around container
