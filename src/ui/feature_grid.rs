//! Pure GTK4 Feature Grid widget.
//!
//! A grid layout for feature toggle widgets with support for expandable menus.

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;

use gtk::prelude::*;
use log::debug;

use crate::menu_state::MenuStore;
use crate::plugin::WidgetFeatureToggle;
use crate::ui::main_window::trigger_window_resize;

/// Pure GTK4 feature grid widget.
pub struct FeatureGridWidget {
    pub root: gtk::Box,
    grid: gtk::Grid,
    menu_store: Arc<MenuStore>,
    /// Current toggle IDs in order, used for diffing
    toggle_ids: Rc<RefCell<Vec<String>>>,
}

impl FeatureGridWidget {
    /// Create a new feature grid with the given toggles.
    pub fn new(items: Vec<Arc<WidgetFeatureToggle>>, menu_store: Arc<MenuStore>) -> Self {
        let root = gtk::Box::new(gtk::Orientation::Vertical, 0);

        let grid = gtk::Grid::builder()
            .column_spacing(12)
            .row_spacing(0)
            .css_classes(["feature-grid"])
            .build();

        let toggle_ids = Rc::new(RefCell::new(Vec::new()));

        let widget = Self {
            root,
            grid,
            menu_store,
            toggle_ids,
        };

        widget.populate_grid(&items);
        widget.root.append(&widget.grid);

        widget
    }

    /// Populate the grid with toggles.
    fn populate_grid(&self, items: &[Arc<WidgetFeatureToggle>]) {
        let cols = 2;

        // Track toggle IDs for diffing
        let mut ids = self.toggle_ids.borrow_mut();
        ids.clear();
        for item in items {
            ids.push(item.id.clone());
        }

        for (pair_idx, pair) in items.chunks(2).enumerate() {
            let grid_row = (pair_idx * 2) as i32;

            // Attach toggles (set widget_name to ID for identification)
            for (col, item) in pair.iter().enumerate() {
                item.el.set_widget_name(&item.id);
                self.grid.attach(&item.el, col as i32, grid_row, 1, 1);
            }

            // Collect menus and menu IDs for this row
            let menus: Vec<_> = pair.iter().filter_map(|item| item.menu.clone()).collect();
            let menu_ids: Vec<String> = pair
                .iter()
                .filter_map(|item| item.menu_id.clone())
                .collect();

            if !menus.is_empty() {
                // Create menu row revealer
                let menu_revealer = gtk::Revealer::builder()
                    .transition_type(gtk::RevealerTransitionType::SlideDown)
                    .reveal_child(false)
                    .build();

                let menu_box = gtk::Box::builder()
                    .orientation(gtk::Orientation::Vertical)
                    .css_classes(["feature-grid-menu-row"])
                    .build();

                for menu in menus {
                    menu_box.append(&menu);
                }

                menu_revealer.set_child(Some(&menu_box));
                self.grid.attach(&menu_revealer, 0, grid_row + 1, cols, 1);

                // Subscribe revealer to menu store
                if !menu_ids.is_empty() {
                    let menu_revealer_clone = menu_revealer.clone();
                    let menu_store_clone = self.menu_store.clone();
                    let menu_ids_clone = menu_ids.clone();
                    self.menu_store.subscribe(move || {
                        let state = menu_store_clone.get_state();
                        // Show revealer if any menu in this row is active
                        let should_be_open = state
                            .active_menu_id
                            .as_ref()
                            .map(|id| menu_ids_clone.contains(id))
                            .unwrap_or(false);
                        menu_revealer_clone.set_reveal_child(should_be_open);
                        if should_be_open {
                            trigger_window_resize();
                        }
                    });

                    // Sync initial state
                    {
                        let state = self.menu_store.get_state();
                        let should_be_open = state
                            .active_menu_id
                            .as_ref()
                            .map(|id| menu_ids.contains(id))
                            .unwrap_or(false);
                        menu_revealer.set_reveal_child(should_be_open);
                    }
                }

                // Connect expand callbacks to this revealer (for backwards compatibility)
                // Preserve any existing callback and chain it with the revealer callback
                for item in pair.iter() {
                    if let Some(ref callback_cell) = item.on_expand_toggled {
                        let revealer = menu_revealer.clone();
                        // Take the existing callback if any
                        let existing_callback = callback_cell.borrow_mut().take();
                        *callback_cell.borrow_mut() = Some(Box::new(move |expanded| {
                            revealer.set_reveal_child(expanded);
                            trigger_window_resize();
                            // Call the original callback if it exists
                            if let Some(ref cb) = existing_callback {
                                cb(expanded);
                            }
                        }));
                    }
                }
            }
        }
    }

    /// Synchronize the grid with a new list of toggles.
    ///
    /// Uses diffing to avoid unnecessary rebuilds:
    /// - If toggle IDs haven't changed, do nothing
    /// - If they have changed, rebuild the grid (menu state is preserved in MenuStore)
    pub fn sync_toggles(&self, items: &[Arc<WidgetFeatureToggle>]) {
        // Check if toggle IDs have changed
        let current_ids = self.toggle_ids.borrow();
        let new_ids: Vec<&str> = items.iter().map(|i| i.id.as_str()).collect();

        let ids_match = current_ids.len() == new_ids.len()
            && current_ids.iter().zip(new_ids.iter()).all(|(a, b)| a == *b);

        if ids_match {
            debug!("[feature_grid] Toggle IDs unchanged, skipping sync");
            return;
        }
        drop(current_ids);

        debug!(
            "[feature_grid] Toggle IDs changed, rebuilding grid ({} -> {} toggles)",
            self.toggle_ids.borrow().len(),
            items.len()
        );

        // Clear the grid
        while let Some(child) = self.grid.first_child() {
            self.grid.remove(&child);
        }

        // Rebuild with new toggles
        self.populate_grid(items);

        trigger_window_resize();
    }

    /// Get a reference to the root widget.
    pub fn widget(&self) -> &gtk::Box {
        &self.root
    }
}
