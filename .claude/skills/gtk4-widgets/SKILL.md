---
name: gtk4-widgets
description: |
  CRITICAL: Use for GTK4 widget questions. Triggers on:
  gtk4 Button, Label, Entry, CheckButton, ToggleButton, Scale,
  SpinButton, TextView, ComboBox, DropDown, ListBox, HeaderBar,
  gtk4 文本输入, gtk4 按钮, gtk4 标签, GTK 部件, gtk-rs 组件
---

# GTK4 Widgets Skill

> **Version:** gtk4 0.10.3 | **Last Updated:** 2025-01-21
>
> Check for updates: https://crates.io/crates/gtk4

You are an expert at the Rust `gtk4` crate widgets. Help users by:
- **Writing code**: Generate widget construction and interaction code
- **Answering questions**: Explain widget properties, signals, and usage

## Documentation

Refer to the local files for detailed documentation:
- `./references/common-widgets.md` - Button, Label, Entry, and other common widgets
- `./references/input-widgets.md` - Text input, selection, and form widgets

## IMPORTANT: Documentation Completeness Check

**Before answering questions, Claude MUST:**

1. Read the relevant reference file(s) listed above
2. If file read fails or file is empty:
   - Inform user: "本地文档不完整，建议运行 `/sync-crate-skills gtk4 --force` 更新文档"
   - Still answer based on SKILL.md patterns + built-in knowledge
3. If reference file exists, incorporate its content into the answer

## Key Patterns

### 1. Button with Click Handler

```rust
use gtk4 as gtk;
use gtk::prelude::*;
use gtk::Button;

let button = Button::with_label("Click me");

button.connect_clicked(|button| {
    button.set_label("Clicked!");
});

// Or with icon
let icon_button = Button::from_icon_name("document-open");
```

### 2. Label with Markup

```rust
use gtk4 as gtk;
use gtk::prelude::*;
use gtk::Label;

// Plain text
let label = Label::new(Some("Hello"));

// With Pango markup
let styled = Label::new(None);
styled.set_markup("<b>Bold</b> and <i>italic</i>");

// With ellipsis for long text
let long_label = Label::new(Some("Very long text..."));
long_label.set_ellipsize(gtk::pango::EllipsizeMode::End);
long_label.set_max_width_chars(20);
```

### 3. Entry (Text Input)

```rust
use gtk4 as gtk;
use gtk::prelude::*;
use gtk::Entry;

let entry = Entry::new();
entry.set_placeholder_text(Some("Enter text..."));

// Get text
entry.connect_changed(|entry| {
    let text = entry.text();
    println!("Current: {}", text);
});

// Handle Enter key
entry.connect_activate(|entry| {
    println!("Submitted: {}", entry.text());
});

// Password entry
let password = Entry::new();
password.set_visibility(false);
password.set_input_purpose(gtk::InputPurpose::Password);
```

### 4. CheckButton and ToggleButton

```rust
use gtk4 as gtk;
use gtk::prelude::*;
use gtk::{CheckButton, ToggleButton};

// CheckButton (checkbox)
let check = CheckButton::with_label("Enable feature");
check.connect_toggled(|check| {
    println!("Checked: {}", check.is_active());
});

// ToggleButton
let toggle = ToggleButton::with_label("Toggle");
toggle.connect_toggled(|toggle| {
    if toggle.is_active() {
        toggle.set_label("ON");
    } else {
        toggle.set_label("OFF");
    }
});
```

## API Reference Table

| Widget | Constructor | Common Signal |
|--------|-------------|---------------|
| `Button` | `with_label()`, `from_icon_name()` | `connect_clicked()` |
| `Label` | `new(Some("text"))` | `connect_label_notify()` |
| `Entry` | `new()` | `connect_changed()`, `connect_activate()` |
| `CheckButton` | `with_label()` | `connect_toggled()` |
| `ToggleButton` | `with_label()` | `connect_toggled()` |
| `Scale` | `with_range()` | `connect_value_changed()` |
| `SpinButton` | `with_range()` | `connect_value_changed()` |
| `Switch` | `new()` | `connect_state_set()` |
| `ComboBoxText` | `new()` | `connect_changed()` |
| `DropDown` | `from_strings()` | `connect_selected_notify()` |

## Deprecated Patterns (Don't Use)

| Deprecated | Correct | Notes |
|------------|---------|-------|
| `ComboBox` | `DropDown` | ComboBox is deprecated in GTK4 |
| `TreeView` | `ColumnView` or `ListView` | TreeView deprecated |
| `FileChooserDialog` | `FileDialog` | Use async file dialogs |
| Manual widget creation | Builder pattern | `Widget::builder().build()` |

## When Writing Code

1. Use builder pattern: `Button::builder().label("Text").build()`
2. For text widgets, use `EditableExt` trait methods: `text()`, `set_text()`
3. Connect signals immediately after widget creation
4. Use `set_margin_all()` for consistent spacing
5. Clone widgets with `widget.clone()` before moving to closures

## When Answering Questions

1. Button, Label, Entry are the most common widgets
2. Use `DropDown` instead of deprecated `ComboBox`
3. `Entry::text()` returns `GString`, convert with `.to_string()`
4. CheckButton is for checkboxes, ToggleButton for toggle switches
5. All widgets support builder pattern via `Widget::builder()`
