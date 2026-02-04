//! Action group widget with popover menu.

use gtk::prelude::*;
use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;

use crate::menu_state::{MenuOp, MenuStore};

use super::action_menu::{ActionMenuOutput, ActionMenuWidget};
use super::dbus::SystemAction;

/// Output events from the action group widget.
#[derive(Debug, Clone)]
pub enum ActionGroupOutput {
    ActionTriggered(SystemAction),
}

/// An action group button with popover menu.
pub struct ActionGroupWidget {
    pub root: gtk::Box,
    _menu_button: gtk::MenuButton,
    on_output: Rc<RefCell<Option<Box<dyn Fn(ActionGroupOutput)>>>>,
}

impl ActionGroupWidget {
    /// Create a new action group widget.
    ///
    /// # Arguments
    /// * `icon_name` - Icon name for the menu button
    /// * `menu` - The action menu widget to display in the popover
    /// * `menu_store` - MenuStore for tracking popover state
    pub fn new(icon_name: &str, menu: ActionMenuWidget, menu_store: Arc<MenuStore>) -> Self {
        let root = gtk::Box::builder()
            .orientation(gtk::Orientation::Horizontal)
            .spacing(0)
            .css_classes(["system-action-group"])
            .build();

        let on_output: Rc<RefCell<Option<Box<dyn Fn(ActionGroupOutput)>>>> =
            Rc::new(RefCell::new(None));

        // Create popover with menu content
        let popover = gtk::Popover::builder().child(&menu.root).build();

        // Generate unique ID for this popover
        let popover_id = uuid::Uuid::new_v4().to_string();

        // Track popover visibility in menu store
        let menu_store_show = menu_store.clone();
        let popover_id_show = popover_id.clone();
        popover.connect_show(move |_| {
            menu_store_show.emit(MenuOp::PopoverOpened(popover_id_show.clone()));
        });

        let menu_store_close = menu_store;
        let popover_id_close = popover_id;
        popover.connect_closed(move |_| {
            menu_store_close.emit(MenuOp::PopoverClosed(popover_id_close.clone()));
        });

        // Create menu button with popover
        let menu_button = gtk::MenuButton::builder()
            .icon_name(icon_name)
            .popover(&popover)
            .css_classes(["system-action-button"])
            .build();

        root.append(&menu_button);

        // Forward menu output to widget output and close popover
        let on_output_clone = on_output.clone();
        let popover_ref = popover.clone();
        menu.connect_output(move |menu_output| {
            let ActionMenuOutput::ActionSelected(action) = menu_output;
            // Close popover after action is selected
            popover_ref.popdown();
            if let Some(ref callback) = *on_output_clone.borrow() {
                callback(ActionGroupOutput::ActionTriggered(action));
            }
        });

        Self {
            root,
            _menu_button: menu_button,
            on_output,
        }
    }

    /// Connect to output events.
    pub fn connect_output<F>(&self, callback: F)
    where
        F: Fn(ActionGroupOutput) + 'static,
    {
        *self.on_output.borrow_mut() = Some(Box::new(callback));
    }
}
