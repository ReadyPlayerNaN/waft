# waft-settings

Standalone GTK4/libadwaita settings application for Waft. Uses `AdwNavigationSplitView` with a categorized sidebar and `gtk::Stack` for page switching. Connects to the Waft daemon via `WaftClient` + `EntityStore` for entity-driven pages.

## Niri Config Writing: KdlConfigFile

The `KdlConfigFile` struct in `src/kdl_config.rs` is the single entry point for reading and writing niri's KDL configuration file (`~/.config/niri/config.kdl`). Both the Startup page and Keyboard Shortcuts page use it.

### Why KdlConfigFile exists

Niri uses KDL v1 syntax. The `kdl` crate v6 defaults to KDL v2 serialization, which produces unquoted string identifiers that niri rejects. Writing a corrupted config breaks the user's compositor with no recovery path beyond the `.bak` backup. `KdlConfigFile` prevents this by enforcing `ensure_v1()` and validation on every save.

### API

```rust
pub struct KdlConfigFile {
    path: PathBuf,
    doc: KdlDocument,
}

impl KdlConfigFile {
    /// Load and parse a KDL file. Returns empty doc if file does not exist.
    /// Returns error if file exists but cannot be parsed.
    pub fn load(path: &Path) -> Result<Self, String>;

    /// Read-only access to the parsed KDL document.
    pub fn doc(&self) -> &KdlDocument;

    /// Mutable access to the parsed KDL document.
    pub fn doc_mut(&mut self) -> &mut KdlDocument;

    /// Remove all top-level nodes with the given name.
    pub fn remove_nodes_by_name(&mut self, name: &str);

    /// Save the document to disk with validation.
    /// Steps: ensure_v1() -> serialize -> validate by re-parsing -> backup -> write.
    pub fn save(&mut self) -> Result<(), String>;
}

/// Default niri config path: ~/.config/niri/config.kdl
pub fn niri_config_path() -> PathBuf;
```

### Save validation pipeline

`KdlConfigFile::save()` performs these steps in order:

1. **`ensure_v1()`** -- Converts all KDL entries to v1 format (quotes string values, converts `#true`/`#false` to `true`/`false`). This is the primary defense against v2 syntax reaching niri.
2. **Serialize** -- `doc.to_string()` produces the output string.
3. **Validate** -- Re-parses the output with `output.parse::<KdlDocument>()`. If parsing fails, the write is aborted and the original file is untouched. This catches any serialization edge case regardless of root cause.
4. **Backup** -- Copies the existing file to `config.kdl.bak` before overwriting.
5. **Write** -- Writes the validated output to disk.

If any step fails, the original config file is preserved.

### How to use KdlConfigFile for new niri config writers

When adding a new settings page that writes to niri's config:

1. Load the config:
   ```rust
   use crate::kdl_config::{KdlConfigFile, niri_config_path};

   let mut config = KdlConfigFile::load(&niri_config_path())?;
   ```

2. Remove old nodes of your type:
   ```rust
   config.remove_nodes_by_name("your-node-name");
   ```

3. Build and append new KDL nodes:
   ```rust
   let mut node = kdl::KdlNode::new("your-node-name");
   node.push(kdl::KdlEntry::new("value"));
   config.doc_mut().nodes_mut().push(node);
   ```

4. Save (validation + backup + write happen automatically):
   ```rust
   config.save()?;
   ```

Do not bypass `KdlConfigFile::save()` to write niri config directly. The `ensure_v1()` call and validation re-parse are required to prevent config corruption.

### Existing consumers

- **`startup/mod.rs`** -- `save_startup_entries()` manages `spawn-at-startup` nodes.
- **`keyboard_shortcuts/mod.rs`** -- `save_binds()` manages the `binds { }` block.

## Pages

| Page | Category | Entity-driven | Description |
|---|---|---|---|
| Bluetooth | Connectivity | Yes | Adapter groups + device lists |
| WiFi | Connectivity | Yes | WiFi adapters + network lists |
| Wired | Connectivity | Yes | Ethernet adapters + connection profiles |
| Appearance | Visual | Yes | Dark mode toggle |
| Display | Visual | Yes | Per-output display controls |
| Wallpaper | Visual | Yes | Wallpaper mode, preview, gallery |
| Audio | Feedback | Yes | Audio device sliders |
| Notifications | Feedback | Yes | Groups, profiles, DND |
| Sounds | Feedback | Yes | Defaults + gallery sections |
| Keyboard | Inputs | Yes | Keyboard layout selection |
| Keyboard Shortcuts | Inputs | No | Direct KDL config editing (niri binds) |
| Weather | Info | Yes | Weather display |
| Plugins | System | Yes | Plugin lifecycle status |
| Services | System | Yes | Systemd user services |
| Startup | System | No | Direct KDL config editing (niri spawn-at-startup) |
