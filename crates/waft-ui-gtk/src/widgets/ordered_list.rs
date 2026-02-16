//! OrderedList widget - container with drag-and-drop reordering via explicit drop zones.
//!
//! A reusable container for ordered lists with drag-and-drop support.
//! Manages OrderedListRow widgets and DropZone widgets between them.
//!
//! # Features
//!
//! - **Explicit drop zones**: Visible insertion targets between items
//! - **Drag handle restriction**: Drag initiated only from the icon, not the entire row
//! - **Visual feedback**: Drop zones highlight on hover, dragged item dims
//! - **Edge case handling**: Proper index calculation for all drop positions

use std::cell::RefCell;
use std::rc::Rc;

use gtk::gdk;
use gtk::prelude::*;

use super::drop_zone::{DropZone, DropZoneProps};
use super::ordered_list_row::{OrderedListRow, OrderedListRowOutput};
use crate::widget_base::WidgetBase;

/// Properties for initializing an ordered list.
#[derive(Clone)]
pub struct OrderedListProps {
    /// CSS classes to apply to the list.
    pub css_classes: Vec<String>,
}

/// Output events from the ordered list.
#[derive(Debug, Clone)]
pub enum OrderedListOutput {
    /// Item reordered: (item_id, from_index, to_index)
    Reordered(String, usize, usize),
}

type OutputCallback = Rc<RefCell<Option<Box<dyn Fn(OrderedListOutput)>>>>;

/// Internal state for the ordered list.
struct OrderedListState {
    /// Ordered list of rows by ID.
    rows: Vec<(String, OrderedListRow)>,
    /// Drop zones (one before each item, one after last item).
    drop_zones: Vec<DropZone>,
    /// Currently dragging item ID.
    dragging_id: Option<String>,
}

/// OrderedList widget - container with drag-and-drop reordering.
#[derive(Clone)]
pub struct OrderedList {
    pub root: gtk::Box,
    state: Rc<RefCell<OrderedListState>>,
    output_cb: OutputCallback,
}

impl OrderedList {
    /// Create a new ordered list.
    pub fn new(props: OrderedListProps) -> Self {
        let root = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .css_classes(["ordered-list"])
            .spacing(1)
            .build();

        // Apply CSS classes
        for class in &props.css_classes {
            root.add_css_class(class);
        }

        let state = Rc::new(RefCell::new(OrderedListState {
            rows: Vec::new(),
            drop_zones: Vec::new(),
            dragging_id: None,
        }));

        let output_cb: OutputCallback = Rc::new(RefCell::new(None));

        Self {
            root,
            state,
            output_cb,
        }
    }

    /// Append an item to the end of the list.
    pub fn append_item(&self, item: &OrderedListRow) {
        let mut s = self.state.borrow_mut();
        let index = s.rows.len();

        // If this is the first item, create initial drop zone
        if s.rows.is_empty() {
            self.insert_drop_zone_at_end(&mut s, 0);
        }

        // Add the row
        self.root.append(&item.root);
        s.rows.push((item.id().to_string(), item.clone()));

        // Create drop zone after this item
        self.insert_drop_zone_at_end(&mut s, index + 1);

        // Connect drag lifecycle events
        self.connect_row_events(item);

        // Update first/last classes
        Self::update_position_classes(&s);
    }

    /// Remove an item by ID.
    pub fn remove_item(&self, id: &str) {
        let mut s = self.state.borrow_mut();
        if let Some(pos) = s.rows.iter().position(|(item_id, _)| item_id == id) {
            // Remove the row widget
            let (_, row) = &s.rows[pos];
            self.root.remove(&row.root);
            s.rows.remove(pos);

            // Rebuild drop zones
            self.rebuild_drop_zones(&mut s);
            Self::update_position_classes(&s);
        }
    }

    /// Clear all items from the list.
    pub fn clear(&self) {
        let mut s = self.state.borrow_mut();

        // Remove all widgets
        let mut child = self.root.first_child();
        while let Some(widget) = child {
            let next = widget.next_sibling();
            self.root.remove(&widget);
            child = next;
        }

        s.rows.clear();
        s.drop_zones.clear();
        s.dragging_id = None;
    }

    /// Get the current order of item IDs.
    pub fn get_order(&self) -> Vec<String> {
        self.state
            .borrow()
            .rows
            .iter()
            .map(|(id, _)| id.clone())
            .collect()
    }

    /// Set the callback for output events.
    pub fn connect_output<F>(&self, callback: F)
    where
        F: Fn(OrderedListOutput) + 'static,
    {
        *self.output_cb.borrow_mut() = Some(Box::new(callback));
    }

    /// Update "first" and "last" CSS classes on rows.
    fn update_position_classes(state: &OrderedListState) {
        let last = state.rows.len().saturating_sub(1);
        for (i, (_, row)) in state.rows.iter().enumerate() {
            if i == 0 {
                row.root.add_css_class("first");
            } else {
                row.root.remove_css_class("first");
            }
            if i == last {
                row.root.add_css_class("last");
            } else {
                row.root.remove_css_class("last");
            }
        }
    }

    /// Connect to a row's drag lifecycle events.
    fn connect_row_events(&self, row: &OrderedListRow) {
        let state_ref = self.state.clone();

        row.connect_output(move |output| match output {
            OrderedListRowOutput::DragBegin(drag_id) => {
                let mut s = state_ref.borrow_mut();
                let drag_index = s.rows.iter().position(|(id, _)| id == &drag_id);
                s.dragging_id = Some(drag_id);
                // Show drop zones except those adjacent to the dragged item
                for (i, zone) in s.drop_zones.iter().enumerate() {
                    let adjacent = drag_index
                        .map(|idx| i == idx || i == idx + 1)
                        .unwrap_or(false);
                    zone.set_visible(!adjacent);
                }
            }
            OrderedListRowOutput::DragEnd(_) => {
                let mut s = state_ref.borrow_mut();
                s.dragging_id = None;
                // Hide all drop zones and remove hover states
                for zone in &s.drop_zones {
                    zone.set_hover(false);
                    zone.set_visible(false);
                }
            }
        });
    }

    /// Append a drop zone at the end of the container.
    fn insert_drop_zone_at_end(&self, state: &mut OrderedListState, index: usize) {
        let zone = DropZone::new(DropZoneProps {
            index,
            visible: false,
        });

        self.setup_drop_zone_target(&zone);
        self.root.append(&zone.root);
        state.drop_zones.push(zone);
    }

    /// Rebuild all drop zones (used after insert/remove).
    fn rebuild_drop_zones(&self, state: &mut OrderedListState) {
        // Remove all existing drop zones
        for zone in &state.drop_zones {
            self.root.remove(&zone.root);
        }
        state.drop_zones.clear();

        if state.rows.is_empty() {
            return;
        }

        // Rebuild: DZ(0), Row(0), DZ(1), Row(1), ..., DZ(n)
        // First, remove all row widgets so we can re-add in order
        for (_, row) in &state.rows {
            self.root.remove(&row.root);
        }

        for (i, (_, row)) in state.rows.iter().enumerate() {
            // Drop zone before this row
            let zone = DropZone::new(DropZoneProps {
                index: i,
                visible: false,
            });
            self.setup_drop_zone_target(&zone);
            self.root.append(&zone.root);
            state.drop_zones.push(zone);

            // The row itself
            self.root.append(&row.root);
        }

        // Final drop zone after last row
        let zone = DropZone::new(DropZoneProps {
            index: state.rows.len(),
            visible: false,
        });
        self.setup_drop_zone_target(&zone);
        self.root.append(&zone.root);
        state.drop_zones.push(zone);
    }

    /// Setup drop target for a drop zone.
    fn setup_drop_zone_target(&self, zone: &DropZone) {
        let drop_target = gtk::DropTarget::new(gtk::glib::Type::STRING, gdk::DragAction::MOVE);

        // Accept: check type and action
        drop_target.connect_accept(|_target, drop| {
            drop.formats().contains_type(gtk::glib::Type::STRING)
                && drop.actions().contains(gdk::DragAction::MOVE)
        });

        // Enter: set hover state
        let zone_clone = zone.clone();
        drop_target.connect_enter(move |_target, _x, _y| {
            zone_clone.set_hover(true);
            gdk::DragAction::MOVE
        });

        // Leave: remove hover state
        let zone_clone = zone.clone();
        drop_target.connect_leave(move |_target| {
            zone_clone.set_hover(false);
        });

        // Drop: compute reorder and emit event
        let zone_index = zone.index();
        let state_ref = self.state.clone();
        let output_ref = self.output_cb.clone();

        drop_target.connect_drop(move |_target, value, _x, _y| {
            // Extract dropped item ID
            let dropped_id = match value.get::<String>() {
                Ok(id) => id,
                Err(e) => {
                    log::warn!("[ordered-list] Failed to get dropped value: {}", e);
                    return false;
                }
            };

            // Find source index
            let s = state_ref.borrow();
            let from_index = match s.rows.iter().position(|(id, _)| id == &dropped_id) {
                Some(idx) => idx,
                None => {
                    log::warn!("[ordered-list] Dropped item ID not found: {}", dropped_id);
                    return false;
                }
            };

            // Target index is the drop zone index
            let target_index = zone_index;

            // Adjust target index if we're dropping after the source
            // (because removing source shifts everything left)
            let actual_target = if from_index < target_index {
                target_index - 1
            } else {
                target_index
            };

            // Don't emit event if dropping at same position
            if from_index == actual_target {
                log::debug!("[ordered-list] Drop at same position, no reorder needed");
                return true;
            }

            // Emit reorder event
            if let Some(ref callback) = *output_ref.borrow() {
                callback(OrderedListOutput::Reordered(
                    dropped_id.clone(),
                    from_index,
                    actual_target,
                ));
            }

            true
        });

        zone.root.add_controller(drop_target);
    }
}

impl WidgetBase for OrderedList {
    fn widget(&self) -> gtk::Widget {
        self.root.clone().upcast::<gtk::Widget>()
    }
}
