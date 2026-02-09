//! Plugin .so discovery and loading logic.
//!
//! This module handles scanning plugin directories for .so files,
//! loading them with libloading, and extracting plugin entry points.

use std::path::{Path, PathBuf};

use log::{debug, error, info, warn};

use crate::PluginMetadata;
use crate::overview::OverviewPlugin;

/// Default plugin directory.
pub const DEFAULT_PLUGIN_DIR: &str = "/usr/lib/waft/plugins";

/// Environment variable to override plugin directory.
pub const PLUGIN_DIR_ENV: &str = "WAFT_PLUGIN_DIR";

/// A loaded plugin .so with its library handle and metadata.
pub struct LoadedPlugin {
    /// The libloading Library handle -- must stay alive while plugin is in use.
    library: libloading::Library,
    /// Plugin metadata from `waft_plugin_metadata()`.
    pub metadata: PluginMetadata,
    /// Path the .so was loaded from.
    pub path: PathBuf,
}

impl LoadedPlugin {
    /// Create an OverviewPlugin from this loaded plugin.
    ///
    /// Calls the `waft_create_overview_plugin` symbol exported by the .so.
    /// Returns `None` if the plugin does not export that symbol or if the
    /// call panics.
    pub fn create_overview_plugin(&self) -> Option<Box<dyn OverviewPlugin>> {
        // Safety: we are loading a known symbol from a dylib that was compiled
        // against the same `waft-plugin-api` crate. The symbol must follow the
        // C ABI and return a heap-allocated trait object pointer created via
        // `Box::into_raw`.
        unsafe {
            let func: libloading::Symbol<unsafe extern "C" fn() -> *mut dyn OverviewPlugin> =
                match self.library.get(b"waft_create_overview_plugin") {
                    Ok(f) => f,
                    Err(_) => {
                        debug!(
                            "Plugin {} does not export waft_create_overview_plugin",
                            self.metadata.id
                        );
                        return None;
                    }
                };

            let raw = match std::panic::catch_unwind(|| func()) {
                Ok(ptr) => ptr,
                Err(_) => {
                    error!(
                        "Plugin {} panicked in waft_create_overview_plugin",
                        self.metadata.id
                    );
                    return None;
                }
            };

            if raw.is_null() {
                warn!(
                    "Plugin {} returned null from waft_create_overview_plugin",
                    self.metadata.id
                );
                return None;
            }

            Some(Box::from_raw(raw))
        }
    }
}

/// Get the plugin directory, checking the env var first, then development paths, then the default.
///
/// Priority:
/// 1. WAFT_PLUGIN_DIR environment variable
/// 2. ./target/debug (if it exists and contains .so files - for development)
/// 3. ./target/release (if it exists and contains .so files - for development)
/// 4. /usr/lib/waft/plugins (production default)
pub fn plugin_dir() -> PathBuf {
    // 1. Check environment variable first
    if let Ok(dir) = std::env::var(PLUGIN_DIR_ENV) {
        return PathBuf::from(dir);
    }

    // 2. Check for development debug build
    let debug_dir = PathBuf::from("./target/debug");
    if is_plugin_dir(&debug_dir) {
        debug!("Using development plugin directory: {}", debug_dir.display());
        return debug_dir;
    }

    // 3. Check for development release build
    let release_dir = PathBuf::from("./target/release");
    if is_plugin_dir(&release_dir) {
        debug!("Using development plugin directory: {}", release_dir.display());
        return release_dir;
    }

    // 4. Fall back to production default
    PathBuf::from(DEFAULT_PLUGIN_DIR)
}

/// Check if a directory exists and contains waft plugin .so files.
fn is_plugin_dir(dir: &Path) -> bool {
    if !dir.is_dir() {
        return false;
    }

    // Check if directory contains at least one libwaft_plugin_*.so file
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            if let Some(name) = entry.file_name().to_str() {
                if name.starts_with("libwaft_plugin_") && name.ends_with(".so") {
                    return true;
                }
            }
        }
    }

    false
}

/// Discover and load all plugin .so files from the given directory.
///
/// For each .so file matching `libwaft_plugin_*.so`:
/// 1. Load with libloading
/// 2. Call `waft_plugin_metadata()` to get metadata
/// 3. Log the rustc version for compatibility awareness
/// 4. Return `LoadedPlugin` if successful
///
/// Plugins that fail to load are logged and skipped.
/// Returns an empty `Vec` if the directory does not exist.
pub fn discover_plugins(dir: &Path) -> Vec<LoadedPlugin> {
    let entries = match std::fs::read_dir(dir) {
        Ok(e) => e,
        Err(err) => {
            if dir == Path::new(DEFAULT_PLUGIN_DIR) {
                debug!("Default plugin directory not found: {err}");
            } else {
                warn!("Cannot read plugin directory {}: {err}", dir.display());
            }
            return Vec::new();
        }
    };

    let mut plugins = Vec::new();

    for entry in entries {
        let entry = match entry {
            Ok(e) => e,
            Err(err) => {
                warn!("Error reading plugin directory entry: {err}");
                continue;
            }
        };

        let path = entry.path();
        let file_name = match path.file_name().and_then(|n| n.to_str()) {
            Some(n) => n.to_owned(),
            None => continue,
        };

        if !file_name.starts_with("libwaft_plugin_") || !file_name.ends_with(".so") {
            continue;
        }

        info!("Loading plugin: {}", path.display());

        match load_plugin_from_path(&path) {
            Ok(loaded) => {
                info!(
                    "Loaded plugin {} v{} (rustc {})",
                    loaded.metadata.name, loaded.metadata.version, loaded.metadata.rustc_version,
                );
                plugins.push(loaded);
            }
            Err(err) => {
                error!("Failed to load plugin {}: {err}", path.display());
            }
        }
    }

    plugins
}

/// Load a single plugin from a .so file path.
fn load_plugin_from_path(path: &Path) -> Result<LoadedPlugin, String> {
    // Safety: we are loading a shared library that follows the waft plugin ABI.
    // The library must export `waft_plugin_metadata` with the correct signature.
    let library = unsafe { libloading::Library::new(path) }
        .map_err(|e| format!("libloading::Library::new failed: {e}"))?;

    // Safety: `waft_plugin_metadata` must be an `extern "C"` fn returning PluginMetadata
    // by value, as generated by the `export_plugin_metadata!` macro.
    let metadata: PluginMetadata = unsafe {
        let func: libloading::Symbol<unsafe extern "C" fn() -> PluginMetadata> = library
            .get(b"waft_plugin_metadata")
            .map_err(|e| format!("missing waft_plugin_metadata symbol: {e}"))?;

        match std::panic::catch_unwind(|| func()) {
            Ok(m) => m,
            Err(_) => return Err("waft_plugin_metadata panicked".into()),
        }
    };

    let host_rustc = current_rustc_version();
    if metadata.rustc_version != host_rustc {
        warn!(
            "Plugin {} compiled with rustc {} but host is {}",
            metadata.name, metadata.rustc_version, host_rustc,
        );
    }

    Ok(LoadedPlugin {
        library,
        metadata,
        path: path.to_owned(),
    })
}

/// Get the current rustc version for compatibility checking.
pub fn current_rustc_version() -> &'static str {
    option_env!("RUSTC_VERSION").unwrap_or("unknown")
}
