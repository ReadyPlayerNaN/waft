# Maintain CDylib Plugin (LEGACY)

**This is the LEGACY plugin architecture.** Only 3 plugins still use it: `notifications`, `eds-agenda`, `sunsetr`. All new plugins MUST use the daemon architecture (see `create-daemon-plugin` skill).

## When to Use

Use this skill only when maintaining the 3 remaining cdylib plugins that have not yet been migrated to daemon architecture.

## CDylib Plugin Overview

CDylib plugins are `.so` files loaded at runtime by waft-overview via `libloading`. They implement the `OverviewPlugin` trait from `waft-plugin-api` and run in-process with the GTK application on the main thread.

## OverviewPlugin Trait Lifecycle

```rust
#[async_trait(?Send)]
impl OverviewPlugin for MyPlugin {
    fn id(&self) -> PluginId;

    // Phase 1: Parse plugin-specific TOML config (before GTK init)
    fn configure(&mut self, settings: &toml::Table) -> Result<()>;

    // Phase 2: Async init with shared resources (before GTK init)
    // ALLOWED: D-Bus connections, channels, pure Rust state
    // FORBIDDEN: Any GTK widget construction
    async fn init(&mut self, resources: &PluginResources) -> Result<()>;

    // Phase 3: GTK widget construction (after GTK init)
    async fn create_elements(
        &mut self,
        app: &gtk::Application,
        menu_store: Rc<MenuStore>,
        registrar: Rc<dyn WidgetRegistrar>,
    ) -> Result<()>;

    // Lifecycle hooks
    fn on_overlay_visible(&mut self, visible: bool) {}
    fn on_session_lock(&mut self) {}
    fn on_session_unlock(&mut self) {}
    async fn cleanup(&mut self) -> Result<()>;
}
```

## PluginResources (Provided by Host)

Plugins receive shared resources from the host. Never create your own:

- `resources.session_dbus` - Session D-Bus connection (`Arc<DbusHandle>`)
- `resources.system_dbus` - System D-Bus connection (`Arc<DbusHandle>`)
- `resources.tokio_handle` - Tokio runtime handle for spawning async tasks

## Critical Requirements

### GTK `unsafe-assume-initialized`

**REQUIRED** in every cdylib plugin's `Cargo.toml`:

```toml
gtk = { version = "0.10", package = "gtk4", features = ["v4_6", "unsafe-assume-initialized"] }
```

Each `.so` gets its own copy of gtk4's `static INITIALIZED: AtomicBool`. The host sets it to `true` via `gtk::init()` but the plugin's copy stays `false`. The `unsafe-assume-initialized` feature skips this check.

### Export Macros

```rust
waft_plugin_api::export_plugin_metadata!("plugin::my-plugin", "MyPlugin", "0.1.0");
waft_plugin_api::export_overview_plugin!(MyPlugin::new());
```

### Threading Rules

- `OverviewPlugin` is `!Send` (uses `#[async_trait(?Send)]`)
- GTK widgets live on the main thread only
- Use `resources.tokio_handle` for spawning async work:
  ```rust
  handle.spawn(async move { /* D-Bus, I/O */ });
  ```
- Never call `tokio::spawn()` directly (no reactor running in plugin context)
- Use `*_with_handle()` methods on `DbusHandle` for D-Bus signal monitoring

## Common Pitfalls

1. **Creating DbusHandle in `init()`** -> "no reactor running" panic. Use `resources.session_dbus` from host.
2. **Calling `tokio::spawn()` directly** -> "no reactor running" panic. Use `handle.spawn()`.
3. **Forgetting `unsafe-assume-initialized`** -> "GTK has not been initialized" panic.
4. **Creating widgets in `init()`** -> "GTK has not been initialized" crash. Create in `create_elements()`.
5. **Using `#[no_mangle]`** in edition 2024 -> compilation error. Use export macros.

## Plugin Loading

CDylib plugins are discovered and loaded in `app.rs`:
1. `waft_plugin_api::loader::discover_plugins()` scans `WAFT_PLUGIN_DIR` (or `/usr/lib/waft/plugins/`)
2. Loads `.so` files via `libloading`
3. Calls `waft_plugin_metadata()` and `waft_create_overview_plugin()`
4. Registers into `PluginRegistry`

## Build & Test

```bash
# Build
cargo build -p waft-plugin-my-plugin

# Verify .so symbols
nm -D target/debug/libwaft_plugin_my_plugin.so | grep waft

# Run with cdylib plugins
WAFT_PLUGIN_DIR=./target/debug cargo run -p waft-overview
```

## Migration Path

These plugins should eventually be migrated to daemon architecture. Key differences:
- Daemon: `PluginDaemon` trait (Send + Sync), runs in own process, no GTK dependency
- CDylib: `OverviewPlugin` trait (!Send), runs in-process, has GTK access
- Migration requires extracting domain logic from GTK widgets into declarative Widget protocol
