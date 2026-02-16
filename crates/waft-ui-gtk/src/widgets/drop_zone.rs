//! DropZone widget - insertion target for drag-and-drop.
//!
//! Shows where an item will be inserted when dropped.
//! Appears as a thin gray line that highlights when hovered during drag.

use std::cell::RefCell;
use std::rc::Rc;

use gtk::prelude::*;

use crate::css::{add_class, remove_class};
use crate::widget_base::WidgetBase;

/// Properties for initializing a drop zone.
#[derive(Clone)]
pub struct DropZoneProps {
    /// Index where items will be inserted if dropped here.
    pub index: usize,
    /// Whether the drop zone is visible.
    pub visible: bool,
}

/// DropZone widget - thin line showing where items can be dropped.
#[derive(Clone)]
pub struct DropZone {
    pub root: gtk::Box,
    index: usize,
    visible: Rc<RefCell<bool>>,
    hover: Rc<RefCell<bool>>,
}

impl DropZone {
    /// Create a new drop zone.
    pub fn new(props: DropZoneProps) -> Self {
        let root = gtk::Box::builder()
            .orientation(gtk::Orientation::Horizontal)
            .css_classes(["drop-zone"])
            .build();

        if props.visible {
            add_class(&root, "visible");
        }

        let visible = Rc::new(RefCell::new(props.visible));
        let hover = Rc::new(RefCell::new(false));

        Self {
            root,
            index: props.index,
            visible,
            hover,
        }
    }

    /// Get the insert index for this drop zone.
    pub fn index(&self) -> usize {
        self.index
    }

    /// Set the index for this drop zone.
    pub fn set_index(&mut self, index: usize) {
        self.index = index;
    }

    /// Show or hide the drop zone.
    pub fn set_visible(&self, visible: bool) {
        *self.visible.borrow_mut() = visible;
        remove_class(&self.root, "visible");
        if visible {
            add_class(&self.root, "visible");
        }
    }

    /// Set hover state (highlighted when true).
    pub fn set_hover(&self, hover: bool) {
        *self.hover.borrow_mut() = hover;
        self.update_css_classes();
    }

    fn update_css_classes(&self) {
        crate::css::apply_state_classes(
            &self.root,
            Some("drop-zone"),
            &[("hover", *self.hover.borrow())],
        );
    }
}

impl WidgetBase for DropZone {
    fn widget(&self) -> gtk::Widget {
        self.root.clone().upcast::<gtk::Widget>()
    }
}
