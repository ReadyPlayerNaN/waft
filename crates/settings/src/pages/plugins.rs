//! Plugins settings page -- smart container.
//!
//! Subscribes to `EntityStore` for `plugin-status` entity type. On entity
//! changes, reconciles the list of plugin rows showing lifecycle state.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use adw::prelude::*;
use waft_client::EntityStore;
use waft_protocol::Urn;
use waft_protocol::entity::plugin::{self, PluginStatus};

use crate::i18n::t;
use crate::plugins::plugin_row::{PluginRow, PluginRowProps};
use crate::search_index::SearchIndex;

/// Smart container for the Plugins settings page.
pub struct PluginsPage {
    pub root: gtk::Box,
}

/// Internal mutable state for the Plugins page.
struct PluginsPageState {
    plugin_rows: HashMap<String, PluginRow>,
    /// Sorted keys for stable row ordering.
    sorted_names: Vec<String>,
    list_box: gtk::ListBox,
    empty_state: adw::StatusPage,
    group: adw::PreferencesGroup,
}

impl PluginsPage {
    pub fn new(entity_store: &Rc<EntityStore>, search_index: &Rc<RefCell<SearchIndex>>) -> Self {
        let root = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .spacing(24)
            .margin_top(24)
            .margin_bottom(24)
            .margin_start(12)
            .margin_end(12)
            .build();

        let empty_state = adw::StatusPage::builder()
            .icon_name("application-x-addon-symbolic")
            .title(t("plugins-no-plugins"))
            .description(t("plugins-no-plugins-desc"))
            .visible(false)
            .build();
        root.append(&empty_state);

        let group = adw::PreferencesGroup::builder()
            .title(t("plugins-title"))
            .visible(false)
            .build();

        let list_box = gtk::ListBox::builder()
            .selection_mode(gtk::SelectionMode::None)
            .css_classes(["boxed-list"])
            .build();
        group.add(&list_box);
        root.append(&group);

        // Register search entries
        {
            let mut idx = search_index.borrow_mut();
            let page_title = t("settings-plugins");
            idx.add_section("plugins", &page_title, &t("plugins-title"), "plugins-title", &group);
        }

        let state = Rc::new(RefCell::new(PluginsPageState {
            plugin_rows: HashMap::new(),
            sorted_names: Vec::new(),
            list_box,
            empty_state,
            group,
        }));

        // Subscribe to plugin-status changes
        {
            let store = entity_store.clone();
            let state = state.clone();
            entity_store.subscribe_type(plugin::ENTITY_TYPE, move || {
                let plugins: Vec<(Urn, PluginStatus)> =
                    store.get_entities_typed(plugin::ENTITY_TYPE);
                log::debug!(
                    "[plugins-page] Subscription triggered: {} plugins",
                    plugins.len()
                );
                Self::reconcile(&state, &plugins);
            });
        }

        // Trigger initial reconciliation with cached data
        {
            let store = entity_store.clone();
            let state = state.clone();
            gtk::glib::idle_add_local_once(move || {
                let plugins: Vec<(Urn, PluginStatus)> =
                    store.get_entities_typed(plugin::ENTITY_TYPE);
                if !plugins.is_empty() {
                    log::debug!(
                        "[plugins-page] Initial reconciliation: {} plugins",
                        plugins.len()
                    );
                    Self::reconcile(&state, &plugins);
                }
            });
        }

        Self { root }
    }

    /// Reconcile the plugin row list with current entity data.
    fn reconcile(state: &Rc<RefCell<PluginsPageState>>, plugins: &[(Urn, PluginStatus)]) {
        let mut state = state.borrow_mut();

        // Build sorted list of plugin names for stable ordering
        let mut current_names: Vec<String> = plugins.iter().map(|(_, p)| p.name.clone()).collect();
        current_names.sort();
        current_names.dedup();

        let mut seen = std::collections::HashSet::new();

        for (_, plugin) in plugins {
            seen.insert(plugin.name.clone());

            let props = PluginRowProps {
                name: plugin.name.clone(),
                state: plugin.state.clone(),
                entity_types: plugin.entity_types.clone(),
            };

            if let Some(existing) = state.plugin_rows.get(&plugin.name) {
                existing.apply_props(&props);
            } else {
                let row = PluginRow::new(&props);
                // Insert in sorted position
                let pos = current_names
                    .iter()
                    .position(|n| n == &plugin.name)
                    .unwrap_or(0);
                state.list_box.insert(&row.root, pos as i32);
                state.plugin_rows.insert(plugin.name.clone(), row);
            }
        }

        // Remove rows no longer present
        let to_remove: Vec<String> = state
            .plugin_rows
            .keys()
            .filter(|k| !seen.contains(*k))
            .cloned()
            .collect();

        for key in to_remove {
            if let Some(row) = state.plugin_rows.remove(&key) {
                state.list_box.remove(&row.root);
            }
        }

        state.sorted_names = current_names;

        // Toggle empty state vs list visibility
        let has_plugins = !state.plugin_rows.is_empty();
        state.group.set_visible(has_plugins);
        state.empty_state.set_visible(!has_plugins);
    }
}
