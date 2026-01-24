# Plugin Architecture

This document describes the plugin architecture used in sacrebleui.

## Plugin Trait

All plugins implement the `Plugin` trait defined in `src/plugin.rs`:

```rust
#[async_trait(?Send)]
pub trait Plugin {
    fn id(&self) -> PluginId;
    fn configure(&mut self, settings: &toml::Table) -> Result<()>;
    async fn init(&mut self) -> Result<()>;
    async fn create_elements(&mut self) -> Result<()>;
    async fn cleanup(&mut self) -> Result<()>;
    fn get_widgets(&self) -> Vec<Arc<Widget>>;
    fn get_feature_toggles(&self) -> Vec<Arc<WidgetFeatureToggle>>;
}
```

### Lifecycle Methods

1. **`id()`** - Returns a unique plugin identifier (e.g., `plugin::clock`)
2. **`configure()`** - Called with settings from the TOML config file
3. **`init()`** - Async initialization (e.g., DBus connections)
4. **`create_elements()`** - Create GTK widgets and start background tasks
5. **`cleanup()`** - Clean up resources when shutting down
6. **`get_widgets()`** - Return widgets to be placed in slots
7. **`get_feature_toggles()`** - Return toggle widgets for the feature grid

## Widget Slots

Widgets can be placed in different UI slots:

- **`Slot::Header`** - Top header bar (clock, weather, etc.)
- **`Slot::Info`** - Information display area
- **`Slot::Controls`** - Control widgets area

Each widget has a `weight` that determines ordering within its slot (lower = earlier).

## Configuration Format

Plugins are configured in `~/.config/sacrebleui/config.toml`:

```toml
[[plugins]]
id = "plugin::clock"
on_click = "gnome-calendar"

[[plugins]]
id = "plugin::weather"
latitude = 50.0755
longitude = 14.4378
units = "celsius"
update_interval = 600
```

## Registration Pattern

Plugins are registered in `src/app.rs`:

```rust
if config.is_plugin_enabled("plugin::weather") {
    let mut plugin = WeatherPlugin::new();
    if let Some(settings) = config.get_plugin_settings("plugin::weather") {
        plugin.configure(settings)?;
    }
    registry.register(plugin);
}
```

## Background Tasks

For periodic updates, use `glib::timeout_add_local`:

```rust
glib::timeout_add_local(Duration::from_secs(interval), move || {
    // Update logic
    glib::ControlFlow::Continue
});
```

For async operations (e.g., HTTP requests), use `glib::spawn_future_local`:

```rust
glib::spawn_future_local(async move {
    let result = some_async_operation().await;
    // Update widget with result
});
```

## UI Components

Pure GTK4 widgets are defined in `src/ui/`. Each widget typically has:

- A struct holding GTK widget references
- A constructor (`new()`)
- Update methods to change state
- Output callbacks for events

Example pattern:

```rust
pub struct MyWidget {
    pub root: gtk::Box,
    label: gtk::Label,
}

impl MyWidget {
    pub fn new() -> Self {
        // Build widgets
    }

    pub fn update(&self, data: &MyData) {
        // Update widget state
    }
}
```

## Available Plugins

- **`plugin::clock`** - Date and time display
- **`plugin::darkman`** - Dark mode toggle (DBus integration)
- **`plugin::sunsetr`** - Sunset/sunrise information
- **`plugin::notifications`** - Notification center
- **`plugin::weather`** - Current weather conditions
