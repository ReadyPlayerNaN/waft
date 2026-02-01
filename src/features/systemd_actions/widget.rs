//! Action group widget with expandable menu.

use gtk::prelude::*;
use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;

use super::action_menu::{ActionMenuOutput, ActionMenuWidget};
use super::dbus::SystemAction;
use crate::menu_state::{MenuOp, MenuStore};
use crate::ui::menu_chevron::{MenuChevronProps, MenuChevronWidget};

/// Output events from the action group widget.
#[derive(Debug, Clone)]
pub enum ActionGroupOutput {
    ActionTriggered(SystemAction),
}

/// An action group button with expandable menu.
pub struct ActionGroupWidget {
    pub root: gtk::Box,
    _menu_revealer: gtk::Revealer,
    _menu_chevron: MenuChevronWidget,
    on_output: Rc<RefCell<Option<Box<dyn Fn(ActionGroupOutput)>>>>,
}

impl ActionGroupWidget {
    /// Create a new action group widget.
    ///
    /// # Arguments
    /// * `icon_name` - Icon name for the main button
    /// * `menu` - The action menu widget to display when expanded
    /// * `menu_id` - Unique identifier for menu coordination
    /// * `menu_store` - Shared menu store for coordinating single-open behavior
    pub fn new(
        icon_name: &str,
        menu: ActionMenuWidget,
        menu_id: String,
        menu_store: Arc<MenuStore>,
    ) -> Self {
        let root = gtk::Box::builder()
            .orientation(gtk::Orientation::Horizontal)
            .spacing(0)
            .css_classes(["system-action-group"])
            .build();

        let on_output: Rc<RefCell<Option<Box<dyn Fn(ActionGroupOutput)>>>> =
            Rc::new(RefCell::new(None));

        // Main button area (icon)
        let main_button = gtk::Button::builder()
            .css_classes(["system-action-button"])
            .build();

        let icon = gtk::Image::builder().icon_name(icon_name).build();
        main_button.set_child(Some(&icon));

        // Make main button non-interactive for now (could add shortcuts later)
        main_button.set_sensitive(false);

        // Expand button with chevron
        let menu_chevron = MenuChevronWidget::new(MenuChevronProps { expanded: false });
        let expand_button = gtk::Button::builder()
            .css_classes(["expand-button"])
            .build();
        expand_button.set_child(Some(&menu_chevron.root));

        // Menu revealer for slide-down animation
        let menu_revealer = gtk::Revealer::builder()
            .transition_type(gtk::RevealerTransitionType::SlideDown)
            .reveal_child(false)
            .build();
        menu_revealer.set_child(Some(&menu.root));

        // Layout: [main_button][expand_button]
        //         [menu_revealer (full width)]
        let top_row = gtk::Box::builder()
            .orientation(gtk::Orientation::Horizontal)
            .spacing(0)
            .build();
        top_row.append(&main_button);
        top_row.append(&expand_button);

        let container = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .spacing(0)
            .build();
        container.append(&top_row);
        container.append(&menu_revealer);

        root.append(&container);

        // Connect expand button to toggle menu
        let menu_id_clone = menu_id.clone();
        let menu_store_clone = menu_store.clone();
        expand_button.connect_clicked(move |_| {
            // Check current state in scoped block to release READ lock before emit
            let is_open = {
                let current_state = menu_store_clone.get_state();
                current_state.active_menu_id.as_ref() == Some(&menu_id_clone)
            }; // READ lock released here

            // Now safe to take WRITE lock via emit
            if is_open {
                menu_store_clone.emit(MenuOp::CloseMenu(menu_id_clone.clone()));
            } else {
                menu_store_clone.emit(MenuOp::OpenMenu(menu_id_clone.clone()));
            }
        });

        // Subscribe to menu store updates
        let menu_id_for_subscription = menu_id.clone();
        let root_for_css = root.clone();
        let menu_chevron_for_update = menu_chevron.clone();
        let menu_revealer_for_update = menu_revealer.clone();
        let menu_store_for_subscription = menu_store.clone();

        menu_store.subscribe(move || {
            let state = menu_store_for_subscription.get_state();
            let should_be_open = state.active_menu_id.as_ref() == Some(&menu_id_for_subscription);

            // Update revealer
            menu_revealer_for_update.set_reveal_child(should_be_open);

            // Update chevron
            menu_chevron_for_update.set_expanded(should_be_open);

            // Update CSS class
            if should_be_open {
                root_for_css.add_css_class("expanded");
            } else {
                root_for_css.remove_css_class("expanded");
            }
        });

        // Forward menu output to widget output
        let on_output_clone = on_output.clone();
        menu.connect_output(move |menu_output| {
            let ActionMenuOutput::ActionSelected(action) = menu_output;
            if let Some(ref callback) = *on_output_clone.borrow() {
                callback(ActionGroupOutput::ActionTriggered(action));
            }
        });

        Self {
            root,
            _menu_revealer: menu_revealer,
            _menu_chevron: menu_chevron,
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
