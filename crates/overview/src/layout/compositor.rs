//! Widget compositors that manage how SlotItems are rendered in a layout slot.

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

/// Renders SlotItems as a vertical stack (plain box).
///
/// Used for `<Widget>` placeholders and `<Unmatched>`.
pub struct StackCompositor {
    root: gtk::Box,
}

impl StackCompositor {
    pub fn new() -> Self {
        let root = gtk::Box::new(gtk::Orientation::Vertical, 12);
        Self { root }
    }
}

impl WidgetCompositor for StackCompositor {
    fn widget(&self) -> &gtk::Widget {
        self.root.upcast_ref()
    }

    fn sync(&self, items: &[SlotItem]) {
        sync_items_to_box(&self.root, items);
    }
}

/// Syncs a list of SlotItems into a gtk::Box using diffing.
///
/// - Widgets present in both old and new lists are kept in place
/// - Only widgets no longer present are removed
/// - Only new widgets are added
/// - Reordering uses `reorder_child_after()` to avoid remounting
fn sync_items_to_box(container: &gtk::Box, items: &[SlotItem]) {
    // Build set of new IDs
    let new_ids: HashSet<&str> = items.iter().map(|item: &SlotItem| item.id()).collect();

    // Collect current children by widget_name (which stores the ID)
    let mut current_children: Vec<(String, gtk::Widget)> = Vec::new();
    let mut child = container.first_child();
    while let Some(widget) = child {
        let id = widget.widget_name().to_string();
        let next = widget.next_sibling();
        current_children.push((id, widget));
        child = next;
    }

    // Remove widgets no longer present
    for (id, widget) in &current_children {
        if !new_ids.contains(id.as_str()) {
            container.remove(widget);
            debug!("[compositor] Removed widget: {}", id);
        }
    }

    // Build set of remaining IDs
    let current_ids: HashSet<String> = current_children
        .iter()
        .filter(|(id, _)| new_ids.contains(id.as_str()))
        .map(|(id, _)| id.clone())
        .collect();

    // Add new widgets and reorder
    let mut prev_widget: Option<gtk::Widget> = None;
    for item in items.iter() {
        let id: &str = item.id();

        if current_ids.contains(id) {
            // Widget exists - find it and reorder if needed
            let mut child = container.first_child();
            while let Some(widget) = child {
                if widget.widget_name() == id {
                    if let Some(ref prev) = prev_widget {
                        if widget.prev_sibling().as_ref() != Some(prev) {
                            container.reorder_child_after(&widget, Some(prev));
                        }
                    } else if widget.prev_sibling().is_some() {
                        container.reorder_child_after(&widget, None::<&gtk::Widget>);
                    }
                    prev_widget = Some(widget);
                    break;
                }
                child = widget.next_sibling();
            }
        } else {
            // New widget - set widget_name and add
            let el = item.el().clone();
            el.set_widget_name(id);
            if let Some(ref prev) = prev_widget {
                container.insert_child_after(&el, Some(prev));
            } else {
                container.prepend(&el);
            }
            prev_widget = Some(el);
            debug!("[compositor] Added widget: {}", id);
        }
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
