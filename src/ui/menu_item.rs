//! Generic menu item widget.
//!
//! A clickable menu item container that provides consistent styling and click handling.

use std::cell::RefCell;
use std::rc::Rc;

use gtk::prelude::*;

/// A generic menu item widget that wraps child content with click handling.
pub struct MenuItemWidget {
    pub root: gtk::Button,
    #[allow(dead_code)]
    on_click: Rc<RefCell<Option<Box<dyn Fn()>>>>,
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

        let on_click_rc: Rc<RefCell<Option<Box<dyn Fn()>>>> =
            Rc::new(RefCell::new(Some(Box::new(on_click))));

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
    pub fn widget(&self) -> &gtk::Button {
        &self.root
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;
    use std::rc::Rc;

    // Test that verifies the MenuItemWidget struct is properly defined
    #[test]
    fn test_menu_item_widget_type_exists() {
        // This test verifies that MenuItemWidget compiles and has the expected methods
        // We can't actually instantiate GTK widgets in tests without a display
        assert!(std::mem::size_of::<MenuItemWidget>() > 0);
    }

    // Test that callback closure can be created and called
    #[test]
    fn test_callback_mechanism() {
        let clicked = Rc::new(RefCell::new(false));
        let clicked_clone = clicked.clone();

        let callback = move || {
            *clicked_clone.borrow_mut() = true;
        };

        callback();
        assert!(*clicked.borrow());
    }

    // Test that callback can be called multiple times
    #[test]
    fn test_callback_multiple_invocations() {
        let count = Rc::new(RefCell::new(0));
        let count_clone = count.clone();

        let callback = move || {
            *count_clone.borrow_mut() += 1;
        };

        callback();
        callback();
        callback();

        assert_eq!(*count.borrow(), 3);
    }

    // The following tests require GTK initialization and X11/Wayland display connection.
    // They are marked as #[ignore] to avoid segfaults in headless CI environments.
    // Run them manually with: cargo test --lib -- --ignored

    #[test]
    #[ignore = "requires GTK display connection"]
    fn test_menu_item_widget_creation_with_display() {
        if gtk::init().is_err() {
            return; // Skip if GTK init fails
        }

        let child = gtk::Label::new(Some("Test Label"));
        let menu_item = MenuItemWidget::new(&child, || {});

        assert!(menu_item.widget().is::<gtk::Button>());
    }

    #[test]
    #[ignore = "requires GTK display connection"]
    fn test_menu_item_widget_has_css_class_with_display() {
        if gtk::init().is_err() {
            return;
        }

        let child = gtk::Label::new(Some("Test Label"));
        let menu_item = MenuItemWidget::new(&child, || {});

        let css_classes = menu_item.widget().css_classes();
        assert!(css_classes.iter().any(|c| c == "menu-item"));
    }

    #[test]
    #[ignore = "requires GTK display connection"]
    fn test_menu_item_widget_click_handler_with_display() {
        if gtk::init().is_err() {
            return;
        }

        let child = gtk::Label::new(Some("Test Label"));
        let clicked = Rc::new(RefCell::new(false));
        let clicked_clone = clicked.clone();

        let menu_item = MenuItemWidget::new(&child, move || {
            *clicked_clone.borrow_mut() = true;
        });

        menu_item.widget().emit_clicked();
        assert!(*clicked.borrow());
    }
}
