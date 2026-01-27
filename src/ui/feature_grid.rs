//! Pure GTK4 Feature Grid widget.
//!
//! A grid layout for feature toggle widgets with support for expandable menus.

use std::sync::Arc;

use gtk::prelude::*;

use crate::plugin::WidgetFeatureToggle;
use crate::ui::main_window::trigger_window_resize;

/// Pure GTK4 feature grid widget.
pub struct FeatureGridWidget {
    pub root: gtk::Box,
}

impl FeatureGridWidget {
    /// Create a new feature grid with the given toggles.
    pub fn new(items: Vec<Arc<WidgetFeatureToggle>>) -> Self {
        let root = gtk::Box::new(gtk::Orientation::Vertical, 0);

        let grid = gtk::Grid::builder()
            .column_spacing(12)
            .row_spacing(0)
            .css_classes(["feature-grid"])
            .build();

        let cols = 2;

        for (pair_idx, pair) in items.chunks(2).enumerate() {
            let grid_row = (pair_idx * 2) as i32;

            // Attach toggles
            for (col, item) in pair.iter().enumerate() {
                grid.attach(&item.el, col as i32, grid_row, 1, 1);
            }

            // Collect menus for this row
            let menus: Vec<_> = pair.iter().filter_map(|item| item.menu.clone()).collect();

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
                grid.attach(&menu_revealer, 0, grid_row + 1, cols, 1);

                // Connect expand callbacks to this revealer
                for item in pair.iter() {
                    if let Some(ref callback_cell) = item.on_expand_toggled {
                        let revealer = menu_revealer.clone();
                        *callback_cell.borrow_mut() = Some(Box::new(move |expanded| {
                            revealer.set_reveal_child(expanded);
                            trigger_window_resize();
                        }));
                    }
                }
            }
        }

        root.append(&grid);

        Self { root }
    }

    /// Get a reference to the root widget.
    pub fn widget(&self) -> &gtk::Box {
        &self.root
    }
}
