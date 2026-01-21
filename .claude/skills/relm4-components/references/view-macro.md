# Relm4 view! Macro Reference

## Basic Syntax

```rust
view! {
    widget_name = WidgetType {
        // Property setters
        set_property: value,

        // Signal connections
        connect_signal => MessageVariant,
        connect_signal => |args| { /* closure */ },

        // Child widgets
        ChildWidget {
            // nested properties...
        },

        // Named children
        method: name = &widget_expression,
    }
}
```

## Widget Properties

### Setting Properties

```rust
view! {
    gtk::Window {
        set_title: Some("My App"),
        set_default_size: (400, 300),
        set_resizable: true,
    }
}
```

### Reactive Properties with #[watch]

Properties marked with `#[watch]` update automatically after `update()`:

```rust
view! {
    gtk::Label {
        #[watch]
        set_label: &model.text,

        #[watch]
        set_visible: model.show_label,

        #[watch]
        set_css_classes: &model.css_classes,
    }
}
```

## Signal Connections

### Message Variant (Recommended)

```rust
view! {
    gtk::Button {
        set_label: "Click",
        connect_clicked => Msg::ButtonClicked,
    }
}
```

### Closure with Arguments

```rust
view! {
    gtk::Entry {
        connect_changed[sender] => move |entry| {
            sender.input(Msg::TextChanged(entry.text().to_string()));
        },
    }
}
```

### Cloning Variables

Use `[var1, var2]` syntax to clone variables into closure:

```rust
view! {
    gtk::Button {
        connect_clicked[sender, some_value] => move |_| {
            sender.input(Msg::Clicked(some_value.clone()));
        },
    }
}
```

## Child Widgets

### Implicit Container Add

```rust
view! {
    gtk::Box {
        // Automatically calls container_add()
        gtk::Label { set_label: "First" },
        gtk::Label { set_label: "Second" },
    }
}
```

### Explicit Methods

```rust
view! {
    gtk::Box {
        // Use specific method
        prepend: gtk::Label { set_label: "At start" },
        append: gtk::Label { set_label: "At end" },
    }
}
```

### Named Widgets

Access widgets later by giving them names:

```rust
view! {
    gtk::Box {
        #[name = "my_label"]
        gtk::Label {
            set_label: "Named widget",
        },
    }
}
// Access as widgets.my_label
```

## Conditional Widgets

### #[watch] for Visibility

```rust
view! {
    gtk::Label {
        #[watch]
        set_visible: model.should_show,
    }
}
```

### Conditional Properties

```rust
view! {
    gtk::Button {
        #[watch]
        set_label: if model.active { "Stop" } else { "Start" },

        #[watch]
        set_sensitive: !model.loading,
    }
}
```

## Widget Templates

### Defining a Template

```rust
#[relm4::widget_template]
impl WidgetTemplate for MyButton {
    view! {
        gtk::Button {
            set_css_classes: &["suggested-action"],
            set_margin_all: 5,
        }
    }
}
```

### Using Templates

```rust
view! {
    gtk::Box {
        #[template]
        MyButton {
            set_label: "Custom Button",
        },
    }
}
```

## Common Patterns

### Box Layout

```rust
view! {
    gtk::Box {
        set_orientation: gtk::Orientation::Vertical,
        set_spacing: 10,
        set_margin_all: 10,

        gtk::Label { set_label: "Header" },
        gtk::Entry { set_placeholder_text: Some("Enter text") },
        gtk::Button { set_label: "Submit" },
    }
}
```

### HeaderBar

```rust
view! {
    gtk::Window {
        set_titlebar: Some(&gtk::HeaderBar) {
            pack_start: &gtk::Button {
                set_icon_name: "open-menu-symbolic",
            },
        },

        gtk::Box {
            // window content
        }
    }
}
```

### Grid Layout

```rust
view! {
    gtk::Grid {
        set_row_spacing: 5,
        set_column_spacing: 5,

        attach[0, 0, 1, 1]: &gtk::Label { set_label: "Name:" },
        attach[1, 0, 1, 1]: &gtk::Entry {},
        attach[0, 1, 1, 1]: &gtk::Label { set_label: "Email:" },
        attach[1, 1, 1, 1]: &gtk::Entry {},
    }
}
```

## Tracker Integration

Use `#[track]` for conditional updates based on field changes:

```rust
#[derive(Default)]
#[tracker::track]
struct Model {
    counter: u8,
    text: String,
}

view! {
    gtk::Label {
        // Only update when counter field changed
        #[track = "model.changed(Model::counter())"]
        set_label: &model.counter.to_string(),
    }
}
```

## libadwaita Widgets

With `features = ["libadwaita"]`:

```rust
view! {
    adw::ApplicationWindow {
        set_title: Some("Adwaita App"),

        adw::ToolbarView {
            add_top_bar: &adw::HeaderBar {},

            #[wrap(Some)]
            set_content: &gtk::Box {
                // content
            },
        }
    }
}
```
