//! Generic menu item widget.
//!
//! A clickable menu item container that provides consistent styling and click handling.

use std::cell::RefCell;
use std::rc::Rc;

use gtk::prelude::*;

/// Type alias for the click callback handler.
type ClickCallback = Rc<RefCell<Option<Box<dyn Fn()>>>>;

/// A generic menu item widget that wraps child content with click handling.
pub struct MenuItemWidget {
    pub root: gtk::Button,
    #[allow(dead_code)]
    on_click: ClickCallback,
}

impl MenuItemWidget {
    /// Create a new menu item widget.
    ///
    /// - `child`: The content widget to display (typically a gtk::Box with icon, labels, etc.)
    /// - `on_click`: Callback invoked when the menu item is clicked
    pub fn new<F>(child: &impl IsA<gtk::Widget>, on_click: F) -> Self
    where
        F: Fn() + 'static,
    {
        let root = gtk::Button::builder()
            .hexpand(true)
            .css_classes(["menu-item"])
            .build();

        root.set_child(Some(child));

        let on_click_rc: ClickCallback = Rc::new(RefCell::new(Some(Box::new(on_click))));

        // Connect click handler
        let on_click_ref = on_click_rc.clone();
        root.connect_clicked(move |_| {
            if let Some(ref callback) = *on_click_ref.borrow() {
                callback();
            }
        });

        Self {
            root,
            on_click: on_click_rc,
        }
    }

    /// Get a reference to the root widget.
    #[allow(dead_code)]
    pub fn widget(&self) -> &gtk::Button {
        &self.root
    }
}
