//! Pure GTK4 Feature Grid widget.
//!
//! A grid layout for feature toggle widgets.

use std::sync::Arc;

use gtk::prelude::*;

use crate::plugin::WidgetFeatureToggle;

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
        for (i, item) in items.iter().enumerate() {
            let col = (i as i32) % cols;
            let row = (i as i32) / cols;
            grid.attach(&item.el, col, row, 1, 1);
        }

        root.append(&grid);

        Self { root }
    }

    /// Get a reference to the root widget.
    pub fn widget(&self) -> &gtk::Box {
        &self.root
    }
}
