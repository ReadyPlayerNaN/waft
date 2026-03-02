---
name: waft-coding-conventions
description: Use when writing any Rust code in waft — covers mandatory naming rules (no utils/helpers modules), boolean field naming convention, and icon widget requirements that must be followed in all new code.
---

# Waft Coding Conventions

Three hard rules that apply to every file in this codebase.

## 1. No Generic Module Names (FORBIDDEN)

Never use `utils`, `helpers`, `misc`, or similar vague module/file/directory names. Every module must be named semantically based on what it contains or does.

```rust
// BAD - vague, meaningless
mod wifi_utils;
mod helpers;
mod misc;

// GOOD - semantic, descriptive
mod wifi_icon;          // Contains WiFi icon selection logic
mod signal_strength;    // Signal strength calculations
mod network_scanner;    // Network scanning functionality
```

Applies to: module names (`mod foo`), file names (`foo.rs`), directory names (`src/features/foo/`).

## 2. Boolean Field Naming: State, Not Question

Boolean fields should be named as states/properties, not questions. Reserve the `is_*`/`has_*`/`can_*` prefix for methods/functions that return booleans.

```rust
// BAD - sounds like a function/question
pub struct AudioDevice {
    pub is_input: bool,    // Reads like "is input?"
    pub is_default: bool,  // Reads like "is default?"
}

// GOOD - state/property naming
pub struct AudioDevice {
    pub input: bool,       // "input" answers "Is input?" -> true/false
    pub default: bool,     // "default" answers "Is default?" -> true/false
}

// Functions/methods CAN use "is_" prefix
impl AudioDevice {
    pub fn is_input(&self) -> bool { self.input }  // OK - method asking question
}
```

**Rationale:** Boolean fields are answers to questions, not questions themselves. The `is_*`/`has_*` prefix suggests a method returning bool.

## 3. Icons: Always Use IconWidget (FORBIDDEN: gtk::Image)

Never use `gtk::Image::builder().icon_name(...)` to create icons. Use `waft_ui_gtk::widgets::IconWidget` — it provides theme resolution, fallback handling, and consistent API.

```rust
// BAD
let img = gtk::Image::builder().icon_name("some-icon-symbolic").build();

// GOOD
use waft_ui_gtk::widgets::IconWidget;

IconWidget::from_name("icon-name", pixel_size)       // simple named icon
IconWidget::new(icon_hints, pixel_size)              // multi-source (themed/file/bytes)
```
