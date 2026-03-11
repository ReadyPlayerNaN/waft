# Launcher Localized App Names Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Make waft-launcher display locale-specific app names from `.desktop` files (e.g. `Name[cs]=`) when a matching locale is available, falling back to the base `Name=` value.

**Architecture:** Locale-specific names are parsed in `desktop_file.rs` into a `HashMap<String, String>` stored on `DesktopEntry`. The daemon binary calls `waft_i18n::system_locale()` at startup to pick the best matching name, writing the resolved string into `App::name` before emitting the entity. No protocol change is needed — the `App` struct stays the same.

**Tech Stack:** `waft-plugin-xdg-apps` (parser, daemon), `waft-i18n::system_locale()`, standard `HashMap`.

---

### Task 1: Collect locale-specific names during `.desktop` parsing

**Files:**
- Modify: `plugins/xdg-apps/src/desktop_file.rs`

**Step 1: Write the failing test**

Add inside the `#[cfg(test)]` block, after the existing `strips_exec_field_codes` test:

```rust
const LOCALIZED_DESKTOP: &str = r#"[Desktop Entry]
Type=Application
Name=Firefox Web Browser
Name[cs]=Webový prohlížeč Firefox
Name[de]=Firefox Webbrowser
Icon=firefox
Exec=firefox %u
"#;

#[test]
fn collects_localized_names() {
    let entry = parse_desktop_entry(LOCALIZED_DESKTOP).unwrap();
    assert_eq!(entry.name, "Firefox Web Browser");
    assert_eq!(
        entry.localized_names.get("cs").map(String::as_str),
        Some("Webový prohlížeč Firefox")
    );
    assert_eq!(
        entry.localized_names.get("de").map(String::as_str),
        Some("Firefox Webbrowser")
    );
}

#[test]
fn localized_names_empty_for_unlocalized_entry() {
    let entry = parse_desktop_entry(FIREFOX_DESKTOP).unwrap();
    assert!(entry.localized_names.is_empty());
}
```

**Step 2: Run test to verify it fails**

Run: `cargo test -p waft-plugin-xdg-apps collects_localized_names`

Expected: compile error — `localized_names` field does not exist yet.

**Step 3: Add `localized_names` to `DesktopEntry` and populate it during parsing**

Replace the `DesktopEntry` struct definition and the key-skipping block in `parse_desktop_entry`:

```rust
use std::collections::HashMap;

#[derive(Debug, Clone, PartialEq)]
pub struct DesktopEntry {
    pub name: String,
    pub icon: String,
    pub exec: String,
    pub description: Option<String>,
    pub keywords: Vec<String>,
    /// Locale-specific names from `Name[lang]=` keys, keyed by language tag
    /// (e.g. `"cs"`, `"de"`, `"pt_BR"`).
    pub localized_names: HashMap<String, String>,
}
```

Inside `parse_desktop_entry`, replace the locale-skipping block and add a new local variable:

```rust
// before the `for line in content.lines()` loop, add:
let mut localized_names: HashMap<String, String> = HashMap::new();

// replace the block that reads:
//   if key.contains('[') {
//       continue;
//   }
// with:
if let Some(locale_key) = key.strip_prefix("Name[").and_then(|s| s.strip_suffix(']')) {
    localized_names.insert(locale_key.to_string(), value.to_string());
    continue;
}
if key.contains('[') {
    continue;
}
```

And include `localized_names` in the returned `DesktopEntry`:

```rust
Some(DesktopEntry {
    name,
    icon: if icon.is_empty() {
        "application-x-executable".to_string()
    } else {
        icon
    },
    exec,
    description,
    keywords,
    localized_names,
})
```

**Step 4: Run tests to verify they pass**

Run: `cargo test -p waft-plugin-xdg-apps`

All existing tests plus the two new ones must pass.

---

### Task 2: Add locale-name resolution helper on `DesktopEntry`

**Files:**
- Modify: `plugins/xdg-apps/src/desktop_file.rs`

**Step 1: Write the failing test**

Add inside `#[cfg(test)]`:

```rust
#[test]
fn resolve_name_exact_locale_match() {
    let entry = parse_desktop_entry(LOCALIZED_DESKTOP).unwrap();
    assert_eq!(entry.resolve_name("cs"), "Webový prohlížeč Firefox");
}

#[test]
fn resolve_name_language_only_match() {
    let entry = parse_desktop_entry(LOCALIZED_DESKTOP).unwrap();
    // "cs_CZ" should match the "cs" key
    assert_eq!(entry.resolve_name("cs_CZ"), "Webový prohlížeč Firefox");
}

#[test]
fn resolve_name_bcp47_language_only_match() {
    let entry = parse_desktop_entry(LOCALIZED_DESKTOP).unwrap();
    // BCP47 "cs-CZ" should also match the "cs" key
    assert_eq!(entry.resolve_name("cs-CZ"), "Webový prohlížeč Firefox");
}

#[test]
fn resolve_name_falls_back_to_base_name() {
    let entry = parse_desktop_entry(LOCALIZED_DESKTOP).unwrap();
    assert_eq!(entry.resolve_name("ja"), "Firefox Web Browser");
}

#[test]
fn resolve_name_empty_locale_falls_back() {
    let entry = parse_desktop_entry(LOCALIZED_DESKTOP).unwrap();
    assert_eq!(entry.resolve_name(""), "Firefox Web Browser");
}
```

**Step 2: Run test to verify it fails**

Run: `cargo test -p waft-plugin-xdg-apps resolve_name`

Expected: compile error — `resolve_name` method does not exist.

**Step 3: Implement `resolve_name` on `DesktopEntry`**

Add after the struct definition (before `parse_desktop_entry`):

```rust
impl DesktopEntry {
    /// Return the best available display name for the given locale string.
    ///
    /// Resolution order:
    /// 1. Exact match on the full locale tag as-is (e.g. `"cs_CZ"` or `"cs-CZ"`).
    /// 2. Language-only prefix before the first `-` or `_` separator (e.g. `"cs"`).
    /// 3. Base `name` field.
    pub fn resolve_name(&self, locale: &str) -> &str {
        // 1. Exact match
        if let Some(name) = self.localized_names.get(locale) {
            return name;
        }
        // 2. Language-only prefix (split on '-' or '_')
        let lang = locale.split(['-', '_']).next().unwrap_or("");
        if !lang.is_empty() {
            if let Some(name) = self.localized_names.get(lang) {
                return name;
            }
        }
        // 3. Fallback
        &self.name
    }
}
```

**Step 4: Run tests to verify they pass**

Run: `cargo test -p waft-plugin-xdg-apps`

All tests must pass.

---

### Task 3: Resolve locale name in the daemon before emitting entities

**Files:**
- Modify: `plugins/xdg-apps/bin/waft-xdg-apps-daemon.rs`
- Modify: `plugins/xdg-apps/Cargo.toml`

**Step 1: Add `waft-i18n` dependency**

In `plugins/xdg-apps/Cargo.toml`, under `[dependencies]`, add:

```toml
waft-i18n = { path = "../../crates/i18n" }
```

**Step 2: Write a focused integration test in the daemon source**

Add at the bottom of `waft-xdg-apps-daemon.rs` (or in a new `tests/` file — inline is fine here since the binary has no test harness; use a lib test instead):

Because the daemon binary cannot easily host `#[test]` items, add the test to `plugins/xdg-apps/src/desktop_file.rs` alongside the other tests (already the right place — the resolution logic lives there):

```rust
#[test]
fn resolve_name_used_with_system_locale_style_string() {
    // Simulate what the daemon does: obtain a BCP47 locale string and resolve.
    let entry = parse_desktop_entry(LOCALIZED_DESKTOP).unwrap();
    // Pretend system_locale() returned "de-DE"
    let locale = "de-DE";
    assert_eq!(entry.resolve_name(locale), "Firefox Webbrowser");
}
```

Run: `cargo test -p waft-plugin-xdg-apps resolve_name_used_with_system_locale_style_string`

This should pass immediately (the method already handles BCP47 separators). Verify it does before proceeding.

**Step 3: Thread the locale through the daemon plugin**

In `waft-xdg-apps-daemon.rs`, add `use waft_i18n::system_locale;` at the top.

Add a `locale` field to `XdgAppsPlugin`:

```rust
struct XdgAppsPlugin {
    apps: Arc<Mutex<HashMap<String, DiscoveredApp>>>,
    dirs: Vec<PathBuf>,
    locale: String,
}
```

In `XdgAppsPlugin::new()`, detect the locale once at startup:

```rust
fn new() -> Self {
    let dirs = xdg_app_dirs();
    let apps = scan_apps(&dirs);
    let map: HashMap<String, DiscoveredApp> =
        apps.into_iter().map(|a| (a.stem.clone(), a)).collect();
    Self {
        apps: Arc::new(Mutex::new(map)),
        dirs,
        locale: system_locale(),
    }
}
```

Update `get_entities` to use `resolve_name`:

```rust
fn get_entities(&self) -> Vec<Entity> {
    let apps = self.apps.lock_or_recover();

    apps.values()
        .map(|app| {
            let entity_data = entity::app::App {
                name: app.entry.resolve_name(&self.locale).to_string(),
                icon: app.entry.icon.clone(),
                available: true,
                keywords: app.entry.keywords.clone(),
                description: app.entry.description.clone(),
            };
            Entity::new(
                Urn::new("xdg-apps", entity::app::ENTITY_TYPE, &app.stem),
                entity::app::ENTITY_TYPE,
                &entity_data,
            )
        })
        .collect()
}
```

Also fix the manifest dummy instance in `main()` (it must compile with the new field):

```rust
let manifest_plugin = XdgAppsPlugin {
    apps: Arc::new(Mutex::new(HashMap::new())),
    dirs: Vec::new(),
    locale: String::new(),
};
```

**Step 4: Build and run all tests**

Run: `cargo build -p waft-plugin-xdg-apps`

Then: `cargo test -p waft-plugin-xdg-apps`

All tests must pass and the build must be clean with no warnings.

---

### Task 4: Verify end-to-end with a manual smoke test

**Files:** (no code changes)

**Step 1: Build the full workspace**

Run: `cargo build --workspace`

**Step 2: Check locale detection**

```bash
WAFT_DAEMON_DIR=./target/debug cargo run --bin waft-xdg-apps-daemon -- provides
```

Should print the manifest. No crash means the new `waft-i18n` dependency links correctly.

**Step 3: Inspect entity output**

With a non-English system locale set (or by temporarily overriding `LANG`), run:

```bash
LANG=cs_CZ.UTF-8 WAFT_DAEMON_DIR=./target/debug cargo run --bin waft-xdg-apps-daemon
```

On a system with Czech-localized `.desktop` files the `name` field in emitted entities should appear in Czech. On systems without localized files the output is unchanged (English names).

**Step 4: Run full workspace tests**

Run: `cargo test --workspace`

No regressions.
