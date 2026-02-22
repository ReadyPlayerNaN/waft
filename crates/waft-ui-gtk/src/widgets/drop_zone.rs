//! DropZone widget - insertion target for drag-and-drop.
//!
//! Shows where an item will be inserted when dropped.
//! Appears as a thin gray line that highlights when hovered during drag.

use std::cell::RefCell;
use std::rc::Rc;

use crate::vdom::{Component, RenderCallback, RenderComponent, RenderFn, VNode};
use crate::vdom::primitives::VBox;
use crate::widget_base::WidgetBase;

/// Properties for the drop zone.
#[derive(Clone, PartialEq, Debug)]
pub struct DropZoneProps {
    /// Index where items will be inserted if dropped here.
    pub index: usize,
    /// Whether the drop zone is visible.
    pub visible: bool,
    /// Whether the drop zone is being hovered.
    pub hover: bool,
}

pub enum DropZoneOutput {}

/// Pure render function for the drop zone visual.
pub struct DropZoneRender;

impl RenderFn for DropZoneRender {
    type Props = DropZoneProps;
    type Output = DropZoneOutput;

    fn render(props: &Self::Props, _emit: &RenderCallback<Self::Output>) -> VNode {
        let mut vbox = VBox::horizontal(0).css_class("drop-zone");

        if props.visible {
            vbox = vbox.css_class("visible");
        }

        if props.hover {
            vbox = vbox.css_class("hover");
        }

        VNode::vbox(vbox)
    }
}

/// Type alias for the drop zone component.
pub type DropZoneComponent = RenderComponent<DropZoneRender>;

/// Backward-compatible wrapper for DropZone.
///
/// Provides a stateful interface that tracks index, visible, and hover states
/// and updates the render component accordingly.
#[derive(Clone)]
pub struct DropZone {
    pub root: gtk::Widget,
    inner: Rc<DropZoneComponent>,
    index: Rc<RefCell<usize>>,
    visible: Rc<RefCell<bool>>,
    hover: Rc<RefCell<bool>>,
}

impl DropZone {
    /// Create a new drop zone.
    pub fn new(props: DropZoneProps) -> Self {
        let inner = Rc::new(DropZoneComponent::build(&props));
        let root = inner.widget();

        Self {
            root,
            inner,
            index: Rc::new(RefCell::new(props.index)),
            visible: Rc::new(RefCell::new(props.visible)),
            hover: Rc::new(RefCell::new(props.hover)),
        }
    }

    /// Get the insert index for this drop zone.
    pub fn index(&self) -> usize {
        *self.index.borrow()
    }

    /// Set the index for this drop zone.
    pub fn set_index(&mut self, index: usize) {
        *self.index.borrow_mut() = index;
    }

    /// Show or hide the drop zone.
    pub fn set_visible(&self, visible: bool) {
        *self.visible.borrow_mut() = visible;
        self.update_render();
    }

    /// Set hover state (highlighted when true).
    pub fn set_hover(&self, hover: bool) {
        *self.hover.borrow_mut() = hover;
        self.update_render();
    }

    fn update_render(&self) {
        let props = DropZoneProps {
            index: *self.index.borrow(),
            visible: *self.visible.borrow(),
            hover: *self.hover.borrow(),
        };
        self.inner.update(&props);
    }
}

impl WidgetBase for DropZone {
    fn widget(&self) -> gtk::Widget {
        self.root.clone()
    }
}
