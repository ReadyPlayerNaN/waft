# waft-launcher Design

**Date:** 2026-02-21
**Branch target:** relm4

## Overview

`waft-launcher` is a new GTK4 layer-shell binary that provides a keyboard-driven application launcher. The user opens it via a compositor keybinding (Niri/Sway/Hyprland), types a fuzzy query, and presses Enter or clicks to launch an app. It closes on focus loss, on launch, or on Escape.

The launcher is a consumer of the waft entity ecosystem: a new `waft-xdg-apps-daemon` plugin enumerates installed `.desktop` apps and provides them as `app` entities. The launcher subscribes to all `app` entities (both `xdg-apps` and `internal-apps`), searches them client-side, and dispatches the `open` action through the daemon to launch.

---

## 1. Protocol Changes

### `entity::app::App` — two new optional fields

```rust
pub struct App {
    pub name: String,
    pub icon: String,
    pub available: bool,
    // New fields (backward-compatible; internal-apps leaves these at defaults):
    pub keywords: Vec<String>,       // From .desktop Keywords= (aids fuzzy scoring)
    pub description: Option<String>, // From .desktop Comment= (shown as subtitle in results)
}
```

- `keywords` defaults to `vec![]`
- `description` defaults to `None`
- `internal-apps` plugin does not need to change — serde defaults handle it
- No untagged enum, no breaking change to serialization

### URN for xdg apps

```
xdg-apps/app/{desktop-stem}
```

Examples: `xdg-apps/app/firefox`, `xdg-apps/app/org.gnome.Nautilus`

The stem is the `.desktop` filename without the extension, lowercased for stability.

---

## 2. New Plugin: `plugins/xdg-apps/`

Binary: `waft-xdg-apps-daemon`

### App discovery

Scans XDG data dirs in order:
1. `$XDG_DATA_HOME/applications` (default: `~/.local/share/applications`)
2. Each entry in `$XDG_DATA_DIRS` split by `:` (default: `/usr/local/share:/usr/share`) + `/applications`

For each `.desktop` file found:
- Skip if `Type != Application`
- Skip if `NoDisplay = true` or `Hidden = true`
- Parse: `Name=`, `Icon=`, `Comment=`, `Keywords=` (`;`-separated), `Exec=`
- Strip locale suffixes from keys (use unlocalized values for now; locale-aware lookup is future work)
- Emit as `app` entity with the `available = true` flag

Apps with the same desktop stem in multiple directories follow standard XDG override order (user overrides system).

### Live updates via inotify

Uses the `notify` crate to watch all scanned `.desktop` directories. On create/modify/delete events:
- Re-parse the affected file (or remove the entity if deleted)
- Call `notifier.notify()` with updated entity

No polling. The watcher runs in a `tokio::spawn` task.

### Launch action: `open`

When the launcher triggers `open`:
1. Retrieve the stored `Exec=` value for the app
2. Strip field codes (`%f`, `%F`, `%u`, `%U`, `%d`, `%D`, `%n`, `%N`, `%i`, `%c`, `%k`, `%v`, `%m`)
3. Tokenize and spawn via `std::process::Command`
4. Reap child in a dedicated `std::thread::spawn` (per CLAUDE.md — no zombies)

The `Exec=` value is stored in the plugin's in-memory state, not in the entity (it's an implementation detail, not presentation data).

### Manifest

Uses `handle_provides_described()` with:
- Plugin name: `xdg-apps`
- Display name: `"XDG Applications"`
- Description: `"Enumerates installed applications from XDG .desktop files"`
- Entity type: `app` with full `PropertyDescription` and `ActionDescription` for `open`

---

## 3. New Crate: `crates/launcher/`

Binary: `waft-launcher`

### Window & layer shell

- GTK4 + libadwaita (consistent theming with the rest of waft)
- `gtk4-layer-shell`: `Layer::Overlay`, no edge anchors (centered on screen), floating
- Width: 640px fixed
- Height: auto, constrained to ~480px max via `ScrolledWindow::set_max_content_height(480)`
- Resizing: call `window.set_default_size(640, -1)` after result list changes (layer-shell pattern)
- `KeyboardMode::Exclusive` — captures all keyboard input while open
- Auto-close: `window.connect_is_active_notify(|w| { if !w.is_active() { w.application().unwrap().quit() } })` — exits when compositor removes focus

### Entity subscription

- Connects to the waft daemon using `WaftClient` (same as overview/toasts)
- Subscribes to `entity::app::ENTITY_TYPE`
- Daemon spawns `xdg-apps` and `internal-apps` on demand
- Filters out entities with `available = false`
- Stores results in a `Vec<(Urn, App)>` updated via the `EntityStore` subscription callback

### Search & ranking

**Fuzzy scoring:**
- Match query against `app.name` (weight 1.0) and `app.keywords.join(" ")` (weight 0.5)
- Uses a simple fuzzy scorer: for each character in the query, finds the next occurrence in the target from left to right; score = match_count / target_len (higher is better); bonus for contiguous runs and prefix matches
- No external crate dependency for the fuzzy algorithm (keep it simple)

**Usage ranking:**
- Persisted to `$XDG_DATA_HOME/waft/launcher-usage.json`
  ```json
  { "xdg-apps/app/firefox": { "launches": 42, "last_used_secs": 1708512000 } }
  ```
- On empty query: sort by `launches` desc (then alphabetical as tiebreak)
- On non-empty query: `combined_score = fuzzy_score + (log2(launches + 1) * 0.1)` — usage nudges but doesn't override relevance
- Configurable: `rank_by_usage = false` disables the usage boost entirely

**Config (`~/.config/waft/config.toml`):**
```toml
[launcher]
rank_by_usage = true  # default
max_results = 20      # default
```

### Keyboard navigation

- `gtk::SearchEntry` holds focus at all times (never transfers to the list)
- Key events captured on the `gtk::Window` via `connect_key_pressed`:
  - `Up` → `selection.set_selected(current - 1)` + scroll to item
  - `Down` → `selection.set_selected(current + 1)` + scroll to item
  - `Enter` → launch selected app (write usage, trigger `open` action, quit)
  - `Escape` → quit immediately
- Clicking a result row: `connect_activate` on `gtk::ListView` → launch + quit
- Search entry `activate` (Enter with no explicit selection): launch first result

---

## 4. `waft-ui-gtk` New Components

All are dumb presentational widgets following the `Props`/`Output`/`connect_output` pattern.

### `AppResultRowWidget` (`app_result_row.rs`)

Single result row: 48px `IconWidget` + vertical label stack (bold name + muted description).

```rust
pub struct AppResultRowProps {
    pub name: String,
    pub icon: String,
    pub description: Option<String>,
}
```

No `Output` (events handled at the ListView level). CSS: `app-result-row`. Used as the GTK ListView factory item.

### `SearchBarWidget` (`search_bar.rs`)

Thin wrapper around `gtk::SearchEntry`.

```rust
pub enum SearchBarOutput {
    Changed(String),   // text changed
    Activated,         // Enter pressed (no result selected)
}
```

Exposes `text() -> String` accessor and `connect_output(cb)`. Applies project CSS classes.

### `EmptySearchStateWidget` (`empty_search_state.rs`)

Placeholder shown when a search returns 0 results.

```rust
pub struct EmptySearchStateProps {
    pub query: String,  // shown in message: "No apps matching 'foo'"
}
```

No `Output`. Centered icon (`edit-find-symbolic`) + message label. Hides itself when `query` is empty.

### `SearchResultListWidget` (`search_result_list.rs`)

`gtk::ListView` + `gtk::SingleSelection` wrapper. Generic over item type via a factory closure.

```rust
pub enum SearchResultListOutput {
    SelectionChanged(usize),
    Activated(usize),
}
```

- `set_items(count: usize)` updates the list model item count (the factory closure provides content)
- `selected_index() -> Option<usize>`
- `select(index: usize)` + scroll to item
- Handles Up/Down/Enter key nav internally

### `SearchPaneWidget` (`search_pane.rs`)

Composite widget: `SearchBarWidget` stacked with either `SearchResultListWidget` (when results exist) or `EmptySearchStateWidget` (when zero results).

```rust
pub enum SearchPaneOutput {
    QueryChanged(String),
    QueryActivated,
    ResultSelected(usize),
    ResultActivated(usize),
}
```

The parent provides the result count and factory; `SearchPaneWidget` manages the stack switching. Reusable in settings pages (WiFi network search, Bluetooth device search, sound gallery search).

---

## 5. Ecosystem Connections

```
waft-protocol
  entity::app::App  ← extended with keywords + description

plugins/xdg-apps/
  waft-xdg-apps-daemon  ← new plugin, provides app entities for all .desktop apps

plugins/internal-apps/
  waft-internal-apps-daemon  ← unchanged, provides waft-settings app entity

crates/launcher/
  waft-launcher  ← new binary, subscribes to app entities, fuzzy search + usage ranking

crates/waft-ui-gtk/src/widgets/
  app_result_row.rs        ← new: AppResultRowWidget
  search_bar.rs            ← new: SearchBarWidget
  empty_search_state.rs    ← new: EmptySearchStateWidget
  search_result_list.rs    ← new: SearchResultListWidget
  search_pane.rs           ← new: SearchPaneWidget

Future reuse of SearchPaneWidget:
  crates/settings/ WiFi page  ← network search
  crates/settings/ Bluetooth  ← device search
  crates/settings/ Sounds     ← sound gallery search
```

---

## 6. Out of Scope

- **Wayland app activation**: after spawning, bringing the new window to focus requires compositor-specific protocol (xdg-activation-v1). Not implemented in v1.
- **Locale-aware `.desktop` parsing**: use unlocalized `Name=`, `Comment=`, `Keywords=` for now.
- **Categories / filtering by category**: search covers all categories; no category filter UI in v1.
- **`.desktop` file `MimeType=` / `%u` URL passing**: `open-url` action deferred.
- **SNI / tray apps in launcher**: separate feature.
- **Wayland drag-to-taskbar**: out of scope.
