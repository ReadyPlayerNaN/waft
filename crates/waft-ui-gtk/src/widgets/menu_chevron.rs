//! Reusable menu chevron widget.
//!
//! A chevron button that rotates when expanded, suitable for menus and expandable content.

use std::cell::RefCell;
use std::rc::Rc;

use gtk::prelude::*;

/// Properties for initializing a menu chevron.
#[derive(Debug, Clone)]
pub struct MenuChevronProps {
    pub expanded: bool,
}

/// Pure GTK4 menu chevron widget.
#[derive(Clone)]
pub struct MenuChevronWidget {
    pub root: gtk::Image,
    expanded: Rc<RefCell<bool>>,
}

impl MenuChevronWidget {
    /// Create a new menu chevron widget.
    pub fn new(props: MenuChevronProps) -> Self {
        let root = gtk::Image::builder()
            .icon_name("pan-down-symbolic")
            .pixel_size(16)
            .css_classes(vec!["menu-chevron"])
            .build();

        let expanded = Rc::new(RefCell::new(props.expanded));
        Self::update_css_classes(&root, props.expanded);
        Self { root, expanded }
    }

    /// Update the expanded state.
    pub fn set_expanded(&self, expanded: bool) {
        *self.expanded.borrow_mut() = expanded;
        Self::update_css_classes(&self.root, expanded);
    }

    pub fn update_css_classes(el: &gtk::Image, expanded: bool) {
        el.remove_css_class("expanded");

        if expanded {
            el.add_css_class("expanded");
        }
    }
}

impl crate::widget_base::WidgetBase for MenuChevronWidget {
    fn widget(&self) -> gtk::Widget {
        self.root.clone().upcast()
    }
}
