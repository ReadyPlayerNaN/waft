//! Small, explicit helpers for keyed GTK child lists.
//!
//! Page controllers own the key-to-widget maps and decide when to create,
//! update, or remove entries. This module centralizes the ordering operation
//! so a `HashMap` never determines visible row order.

use gtk::prelude::*;

/// Reorder existing children without recreating them.
///
/// Every widget must already be a child of `parent`. GTK moves an existing
/// child when it is inserted after another sibling, preserving widget identity
/// and signal handlers while making the desired entity order visible.
pub fn reorder_children(
    parent: &impl IsA<gtk::Widget>,
    widgets: impl IntoIterator<Item = gtk::Widget>,
) {
    let mut previous: Option<gtk::Widget> = None;
    for widget in widgets {
        widget.insert_after(parent, previous.as_ref());
        previous = Some(widget);
    }
}
