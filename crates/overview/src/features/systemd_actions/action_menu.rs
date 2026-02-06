//! Action menu widget for displaying system actions.

use gtk::prelude::*;
use std::cell::RefCell;
use std::rc::Rc;

use super::dbus::SystemAction;
use crate::common::Callback;
use crate::ui::menu_item::MenuItemWidget;

/// Output events from the action menu.
#[derive(Debug, Clone)]
pub enum ActionMenuOutput {
    ActionSelected(SystemAction),
}

/// A vertical menu of system action items.
pub struct ActionMenuWidget {
    pub root: gtk::Box,
    on_output: Callback<ActionMenuOutput>,
}

impl ActionMenuWidget {
    /// Create a new session actions menu (Lock, Logout).
    pub fn new_session_menu() -> Self {
        let root = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .spacing(4)
            .css_classes(["system-action-menu"])
            .build();

        let on_output: Callback<ActionMenuOutput> = Rc::new(RefCell::new(None));

        // Lock Session action
        let lock_item = Self::create_menu_item(
            "system-lock-screen-symbolic",
            "Lock Session",
            SystemAction::LockSession,
            on_output.clone(),
        );
        root.append(&lock_item);

        // Logout action
        let logout_item = Self::create_menu_item(
            "system-log-out-symbolic",
            "Logout",
            SystemAction::Terminate,
            on_output.clone(),
        );
        root.append(&logout_item);

        Self { root, on_output }
    }

    /// Create a new power actions menu (Reboot, Shutdown, Suspend).
    pub fn new_power_menu() -> Self {
        let root = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .spacing(4)
            .css_classes(["system-action-menu"])
            .build();

        let on_output: Callback<ActionMenuOutput> = Rc::new(RefCell::new(None));

        // Reboot action
        let reboot_item = Self::create_menu_item(
            "system-reboot-symbolic",
            "Reboot",
            SystemAction::Reboot { interactive: true },
            on_output.clone(),
        );
        root.append(&reboot_item);

        // Shutdown action
        let shutdown_item = Self::create_menu_item(
            "system-shutdown-symbolic",
            "Shutdown",
            SystemAction::PowerOff { interactive: true },
            on_output.clone(),
        );
        root.append(&shutdown_item);

        // Suspend action
        let suspend_item = Self::create_menu_item(
            "media-playback-pause-symbolic",
            "Suspend",
            SystemAction::Suspend { interactive: true },
            on_output.clone(),
        );
        root.append(&suspend_item);

        Self { root, on_output }
    }

    /// Create a menu item with icon, label, and click handler.
    fn create_menu_item(
        icon_name: &str,
        label_text: &str,
        action: SystemAction,
        on_output: Callback<ActionMenuOutput>,
    ) -> gtk::Widget {
        let item_box = gtk::Box::builder()
            .orientation(gtk::Orientation::Horizontal)
            .spacing(12)
            .build();

        let icon = gtk::Image::builder().icon_name(icon_name).build();

        let label = gtk::Label::builder()
            .label(label_text)
            .xalign(0.0)
            .hexpand(true)
            .build();

        item_box.append(&icon);
        item_box.append(&label);

        // Create menu item with click handler
        let menu_item = MenuItemWidget::new(&item_box, {
            let on_output = on_output.clone();
            move || {
                if let Some(ref callback) = *on_output.borrow() {
                    callback(ActionMenuOutput::ActionSelected(action));
                }
            }
        });

        menu_item.root.add_css_class("system-action-row");

        menu_item.root.upcast()
    }

    /// Connect to output events.
    pub fn connect_output<F>(&self, callback: F)
    where
        F: Fn(ActionMenuOutput) + 'static,
    {
        *self.on_output.borrow_mut() = Some(Box::new(callback));
    }
}
