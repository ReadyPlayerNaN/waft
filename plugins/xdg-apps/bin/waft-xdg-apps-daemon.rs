//! XDG Applications plugin -- provides app entities from .desktop files.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use anyhow::{Context, Result};
use notify::{Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use waft_plugin::*;
use waft_protocol::description::*;
use waft_xdg_apps::desktop_file::strip_exec_field_codes;
use waft_xdg_apps::scanner::{scan_apps, xdg_app_dirs, DiscoveredApp};

/// Shared plugin state: current discovered apps indexed by stem.
struct XdgAppsPlugin {
    apps: Arc<Mutex<HashMap<String, DiscoveredApp>>>,
    dirs: Vec<PathBuf>,
}

impl XdgAppsPlugin {
    fn new() -> Self {
        let dirs = xdg_app_dirs();
        let apps = scan_apps(&dirs);
        let map: HashMap<String, DiscoveredApp> =
            apps.into_iter().map(|a| (a.stem.clone(), a)).collect();
        Self {
            apps: Arc::new(Mutex::new(map)),
            dirs,
        }
    }
}

#[async_trait::async_trait]
impl Plugin for XdgAppsPlugin {
    fn get_entities(&self) -> Vec<Entity> {
        let apps = match self.apps.lock() {
            Ok(g) => g,
            Err(e) => {
                log::warn!("[xdg-apps] mutex poisoned, recovering: {e}");
                e.into_inner()
            }
        };

        apps.values()
            .map(|app| {
                let entity_data = entity::app::App {
                    name: app.entry.name.clone(),
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

    async fn handle_action(
        &self,
        urn: Urn,
        action: String,
        _params: serde_json::Value,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let stem = urn.id().to_string();
        match action.as_str() {
            "open" => {
                let exec = {
                    let apps = match self.apps.lock() {
                        Ok(g) => g,
                        Err(e) => e.into_inner(),
                    };
                    apps.get(&stem)
                        .map(|a| strip_exec_field_codes(&a.entry.exec))
                };

                let Some(exec) = exec else {
                    log::warn!("[xdg-apps] open: app '{stem}' not found");
                    return Ok(());
                };

                let parts: Vec<&str> = exec.split_whitespace().collect();
                if parts.is_empty() {
                    log::warn!("[xdg-apps] open: empty exec for '{stem}'");
                    return Ok(());
                }

                let mut cmd = std::process::Command::new(parts[0]);
                for arg in &parts[1..] {
                    cmd.arg(arg);
                }

                match cmd.spawn() {
                    Ok(child) => {
                        std::thread::spawn(move || {
                            let mut child = child;
                            match child.wait() {
                                Ok(status) => {
                                    log::debug!("[xdg-apps] '{stem}' exited: {status}")
                                }
                                Err(e) => {
                                    log::warn!("[xdg-apps] wait failed for '{stem}': {e}")
                                }
                            }
                        });
                    }
                    Err(e) => {
                        log::error!("[xdg-apps] failed to spawn '{stem}': {e}");
                        return Err(Box::new(e));
                    }
                }
            }
            other => {
                log::debug!("[xdg-apps] unknown action: {other}");
            }
        }
        Ok(())
    }

    fn can_stop(&self) -> bool {
        true
    }

    fn describe(&self) -> Option<PluginDescription> {
        Some(PluginDescription {
            name: "xdg-apps".to_string(),
            display_name: "XDG Applications".to_string(),
            description: "Enumerates installed applications from XDG .desktop files".to_string(),
            entity_types: vec![EntityTypeDescription {
                entity_type: entity::app::ENTITY_TYPE.to_string(),
                display_name: "Application".to_string(),
                description: "A launchable application from a .desktop file".to_string(),
                properties: vec![
                    PropertyDescription {
                        name: "name".to_string(),
                        label: "Name".to_string(),
                        description: "Application display name".to_string(),
                        value_type: PropertyValueType::String,
                    },
                    PropertyDescription {
                        name: "icon".to_string(),
                        label: "Icon".to_string(),
                        description: "Themed icon name or file path".to_string(),
                        value_type: PropertyValueType::String,
                    },
                    PropertyDescription {
                        name: "available".to_string(),
                        label: "Available".to_string(),
                        description: "Always true for XDG apps".to_string(),
                        value_type: PropertyValueType::Bool,
                    },
                    PropertyDescription {
                        name: "keywords".to_string(),
                        label: "Keywords".to_string(),
                        description: "Search keywords from Keywords= field".to_string(),
                        value_type: PropertyValueType::String,
                    },
                    PropertyDescription {
                        name: "description".to_string(),
                        label: "Description".to_string(),
                        description: "App description from Comment= field".to_string(),
                        value_type: PropertyValueType::String,
                    },
                ],
                actions: vec![ActionDescription {
                    name: "open".to_string(),
                    label: "Open".to_string(),
                    description: "Launch the application".to_string(),
                    params: vec![],
                }],
            }],
        })
    }
}

fn main() -> Result<()> {
    let manifest_plugin = XdgAppsPlugin {
        apps: Arc::new(Mutex::new(HashMap::new())),
        dirs: Vec::new(),
    };
    if waft_plugin::manifest::handle_provides_described(
        &[entity::app::ENTITY_TYPE],
        "XDG Applications",
        "Enumerates installed applications from XDG .desktop files",
        &manifest_plugin,
    ) {
        return Ok(());
    }

    waft_plugin::init_plugin_logger("info");
    log::info!("[xdg-apps] starting...");

    let rt = tokio::runtime::Runtime::new().context("failed to create tokio runtime")?;
    rt.block_on(async {
        let plugin = XdgAppsPlugin::new();
        let apps_ref = plugin.apps.clone();
        let dirs = plugin.dirs.clone();

        let (runtime, notifier) = PluginRuntime::new("xdg-apps", plugin);

        // Spawn inotify watcher task
        tokio::spawn(async move {
            if let Err(e) = watch_dirs(dirs, apps_ref, notifier).await {
                log::warn!("[xdg-apps] file watcher exited: {e}");
            }
            log::debug!("[xdg-apps] file watcher stopped");
        });

        runtime.run().await?;
        Ok(())
    })
}

async fn watch_dirs(
    dirs: Vec<PathBuf>,
    apps: Arc<Mutex<HashMap<String, DiscoveredApp>>>,
    notifier: EntityNotifier,
) -> Result<()> {
    let (tx, rx) = std::sync::mpsc::channel::<notify::Result<Event>>();

    let mut watcher = RecommendedWatcher::new(tx, notify::Config::default())
        .context("failed to create file watcher")?;

    for dir in &dirs {
        if dir.exists() && let Err(e) = watcher.watch(dir, RecursiveMode::NonRecursive) {
            log::warn!("[xdg-apps] could not watch {dir:?}: {e}");
        }
    }

    // Process events in a blocking thread to avoid blocking the tokio runtime
    tokio::task::spawn_blocking(move || {
        let _watcher = watcher; // Keep watcher alive
        for result in rx {
            match result {
                Ok(event) => {
                    // Only react to structural changes: files created, removed,
                    // or renamed. Skip Access/Open/Close events — those are
                    // generated by scan_apps() itself reading files, which would
                    // create a feedback loop that saturates the CPU.
                    let is_structural = matches!(
                        event.kind,
                        EventKind::Create(_)
                            | EventKind::Remove(_)
                            | EventKind::Modify(_)
                    ) && !matches!(event.kind, EventKind::Access(_));

                    if !is_structural {
                        continue;
                    }

                    let new_apps = scan_apps(&dirs);
                    let new_map: HashMap<String, DiscoveredApp> =
                        new_apps.into_iter().map(|a| (a.stem.clone(), a)).collect();

                    match apps.lock() {
                        Ok(mut guard) => *guard = new_map,
                        Err(e) => {
                            log::warn!("[xdg-apps] mutex poisoned, recovering: {e}");
                            *e.into_inner() = new_map;
                        }
                    }

                    if !notifier.notify() {
                        // Runtime has stopped (CanStop from daemon). Exit the
                        // watcher thread so it doesn't spin forever logging errors.
                        log::debug!("[xdg-apps] runtime stopped, exiting watcher");
                        break;
                    }
                    log::debug!("[xdg-apps] app list refreshed after file system change");
                }
                Err(e) => {
                    log::warn!("[xdg-apps] watcher error: {e}");
                }
            }
        }
        log::debug!("[xdg-apps] watcher channel closed");
    })
    .await
    .context("watcher thread panicked")?;

    Ok(())
}
