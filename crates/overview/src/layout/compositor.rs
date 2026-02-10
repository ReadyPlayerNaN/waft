//! Widget compositors that manage how SlotItems are rendered in a layout slot.

use std::cell::RefCell;
use std::collections::HashSet;
use std::rc::Rc;

use gtk::prelude::*;
use log::debug;

use crate::menu_state::MenuStore;
use crate::plugin::WidgetFeatureToggle;
use crate::plugin_registry::SlotItem;
use crate::ui::feature_grid::FeatureGridWidget;

/// A compositor manages a GTK container and syncs SlotItems into it.
pub trait WidgetCompositor {
    /// The root GTK widget managed by this compositor.
    fn widget(&self) -> &gtk::Widget;

    /// Synchronize the compositor's content with the given items.
    fn sync(&self, items: &[SlotItem]);
}

/// Mounts SlotItems directly into the parent container (fragment mounting).
///
/// Instead of wrapping items in its own box, inserts them as direct siblings
/// in the parent. Uses an invisible anchor widget to track position.
/// This preserves CSS style inheritance from the parent.
pub struct FragmentCompositor {
    anchor: gtk::Widget,
    managed: RefCell<Vec<(String, gtk::Widget)>>,
}

impl FragmentCompositor {
    pub fn new() -> Self {
        let anchor = gtk::Box::new(gtk::Orientation::Vertical, 0);
        anchor.set_visible(false);
        Self {
            anchor: anchor.upcast(),
            managed: RefCell::new(Vec::new()),
        }
    }
}

impl WidgetCompositor for FragmentCompositor {
    fn widget(&self) -> &gtk::Widget {
        &self.anchor
    }

    fn sync(&self, items: &[SlotItem]) {
        let Some(parent) = self
            .anchor
            .parent()
            .and_then(|p| p.downcast::<gtk::Box>().ok())
        else {
            debug!("[compositor] Fragment anchor not mounted in a Box, skipping sync");
            return;
        };

        let new_ids: HashSet<&str> = items.iter().map(|item| item.id()).collect();
        let mut managed = self.managed.borrow_mut();

        // Remove widgets no longer present
        managed.retain(|(id, widget)| {
            if new_ids.contains(id.as_str()) {
                true
            } else {
                parent.remove(widget);
                debug!("[compositor] Removed widget: {}", id);
                false
            }
        });

        // Build lookup of existing managed widgets
        let current_ids: HashSet<&str> = managed.iter().map(|(id, _)| id.as_str()).collect();

        // Add new widgets and reorder after anchor
        let mut prev = self.anchor.clone();
        let mut new_managed: Vec<(String, gtk::Widget)> = Vec::new();

        for item in items {
            let id = item.id();

            if current_ids.contains(id) {
                // Existing widget — reorder if needed
                let widget = &managed.iter().find(|(mid, _)| mid == id).unwrap().1;
                if widget.prev_sibling().as_ref() != Some(&prev) {
                    parent.reorder_child_after(widget, Some(&prev));
                }
                prev = widget.clone();
                new_managed.push((id.to_string(), widget.clone()));
            } else {
                // New widget
                let el = item.el().clone();
                el.set_widget_name(id);
                parent.insert_child_after(&el, Some(&prev));
                prev = el.clone();
                new_managed.push((id.to_string(), el));
                debug!("[compositor] Added widget: {}", id);
            }
        }

        *managed = new_managed;
    }
}

/// Renders SlotItems into a FeatureToggleGrid.
///
/// Filters for `SlotItem::Toggle` items and delegates to `FeatureGridWidget`.
pub struct FeatureToggleGridCompositor {
    grid: FeatureGridWidget,
}

impl FeatureToggleGridCompositor {
    pub fn new(menu_store: Rc<MenuStore>) -> Self {
        let grid = FeatureGridWidget::new(Vec::new(), menu_store);
        Self { grid }
    }
}

impl WidgetCompositor for FeatureToggleGridCompositor {
    fn widget(&self) -> &gtk::Widget {
        self.grid.widget().upcast_ref()
    }

    fn sync(&self, items: &[SlotItem]) {
        let toggles: Vec<Rc<WidgetFeatureToggle>> = items
            .iter()
            .filter_map(|item| {
                if let SlotItem::Toggle(t) = item {
                    Some(t.clone())
                } else {
                    None
                }
            })
            .collect();
        self.grid.sync_toggles(&toggles);
    }
}
