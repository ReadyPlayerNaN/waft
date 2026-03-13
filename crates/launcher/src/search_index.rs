use waft_client::EntityStore;
use waft_protocol::entity;
use waft_protocol::entity::app::App;
use waft_protocol::Urn;

use crate::normalize::{Normalized, normalize_for_search};

pub struct AppSearchEntry {
    pub urn: Urn,
    pub app: App,
    pub name_norm: Normalized,
    /// Keywords joined with space, then normalized.
    pub keywords_norm: Normalized,
}

pub struct WindowSearchEntry {
    pub urn: Urn,
    pub window: entity::window::Window,
    pub title_norm: Normalized,
    pub app_id_norm: Normalized,
}

pub struct SearchIndex {
    pub apps: Vec<AppSearchEntry>,
    pub windows: Vec<WindowSearchEntry>,
}

impl SearchIndex {
    pub fn new() -> Self {
        Self {
            apps: Vec::new(),
            windows: Vec::new(),
        }
    }
}

impl Default for SearchIndex {
    fn default() -> Self {
        Self::new()
    }
}

impl SearchIndex {
    /// Rebuild app entries from the entity store. Called when app entities change.
    pub fn rebuild_apps(&mut self, store: &EntityStore) {
        let raw: Vec<(Urn, App)> = store.get_entities_typed(entity::app::ENTITY_TYPE);
        self.apps = raw
            .into_iter()
            .map(|(urn, app)| {
                let name_norm = normalize_for_search(&app.name);
                let keywords_str = app.keywords.join(" ");
                let keywords_norm = normalize_for_search(&keywords_str);
                AppSearchEntry {
                    urn,
                    app,
                    name_norm,
                    keywords_norm,
                }
            })
            .collect();
    }

    /// Rebuild window entries from the entity store. Called when window entities change.
    pub fn rebuild_windows(&mut self, store: &EntityStore) {
        let raw: Vec<(Urn, entity::window::Window)> =
            store.get_entities_typed(entity::window::ENTITY_TYPE);
        self.windows = raw
            .into_iter()
            .map(|(urn, window)| {
                let title_norm = normalize_for_search(&window.title);
                let app_id_norm = normalize_for_search(&window.app_id);
                WindowSearchEntry {
                    urn,
                    window,
                    title_norm,
                    app_id_norm,
                }
            })
            .collect();
    }

    /// Returns true if no entities are loaded yet.
    pub fn is_empty(&self) -> bool {
        self.apps.is_empty() && self.windows.is_empty()
    }
}
