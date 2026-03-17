//! Plugins settings page -- smart container.
//!
//! Subscribes to `EntityStore` for `plugin-status` entity type. On entity
//! changes, reconciles the list of plugin rows showing lifecycle state.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use waft_client::EntityStore;
use waft_ui_gtk::vdom::Component;
use waft_protocol::Urn;
use waft_protocol::entity::plugin::{self, PluginStatus};

use crate::entity_list_group::EntityListGroup;
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
    list_group: EntityListGroup,
}

impl PluginsPage {
    /// Phase 1: Register static search entries without constructing widgets.
    pub fn register_search(idx: &mut SearchIndex) {
        let page_title = t("settings-plugins");
        idx.add_section_deferred("plugins", &page_title, &t("plugins-title"), "plugins-title");
    }

    pub fn new(entity_store: &Rc<EntityStore>, search_index: &Rc<RefCell<SearchIndex>>) -> Self {
        let root = crate::page_layout::page_root();

        let list_group = EntityListGroup::new(
            &root,
            "application-x-addon-symbolic",
            &t("plugins-no-plugins"),
            &t("plugins-no-plugins-desc"),
            &t("plugins-title"),
        );

        // Backfill search entry widgets
        {
            let mut idx = search_index.borrow_mut();
            idx.backfill_widget("plugins", &t("plugins-title"), None, Some(&list_group.group));
        }

        let state = Rc::new(RefCell::new(PluginsPageState {
            plugin_rows: HashMap::new(),
            sorted_names: Vec::new(),
            list_group,
        }));

        // Subscribe to plugin-status changes (future updates + initial reconciliation)
        crate::subscription::subscribe_entities::<PluginStatus, _>(
            entity_store,
            plugin::ENTITY_TYPE,
            {
                let state = state.clone();
                move |plugins| {
                    log::debug!(
                        "[plugins-page] Reconciling: {} plugins",
                        plugins.len()
                    );
                    Self::reconcile(&state, &plugins);
                }
            },
        );

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
                existing.update(&props);
            } else {
                let row = PluginRow::build(&props);
                state.list_group.insert_sorted(&row.widget(), &plugin.name, &current_names);
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
                state.list_group.list_box.remove(&row.widget());
            }
        }

        state.sorted_names = current_names;
        state.list_group.toggle_visibility(!state.plugin_rows.is_empty());
    }
}
