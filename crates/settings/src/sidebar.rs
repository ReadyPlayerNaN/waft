//! Settings category sidebar.
//!
//! Dumb widget displaying a list of settings categories. Emits
//! `SidebarOutput::Selected` when the user picks a category.

use std::cell::RefCell;
use std::rc::Rc;

use adw::prelude::*;
use waft_ui_gtk::widgets::icon::IconWidget;

/// Output events from the sidebar.
pub enum SidebarOutput {
    /// A category was selected by the user.
    Selected(String),
}

/// Callback type for sidebar output events.
type OutputCallback = Rc<RefCell<Option<Box<dyn Fn(SidebarOutput)>>>>;

/// Category sidebar widget.
pub struct Sidebar {
    pub root: gtk::ListBox,
    output_cb: OutputCallback,
}

impl Sidebar {
    pub fn new() -> Self {
        let list_box = gtk::ListBox::builder()
            .selection_mode(gtk::SelectionMode::Single)
            .css_classes(["navigation-sidebar"])
            .build();

        // Bluetooth row (active)
        let bt_icon = IconWidget::from_name("bluetooth-active-symbolic", 16);
        let bt_row = adw::ActionRow::builder()
            .title("Bluetooth")
            .activatable(true)
            .build();
        bt_row.add_prefix(bt_icon.widget());
        list_box.append(&bt_row);

        // Network row (placeholder, insensitive)
        let net_icon = IconWidget::from_name("network-wireless-symbolic", 16);
        let net_row = adw::ActionRow::builder()
            .title("Network")
            .activatable(true)
            .sensitive(false)
            .build();
        net_row.add_prefix(net_icon.widget());
        list_box.append(&net_row);

        // Display row (placeholder, insensitive)
        let disp_icon = IconWidget::from_name("preferences-desktop-display-symbolic", 16);
        let disp_row = adw::ActionRow::builder()
            .title("Display")
            .activatable(true)
            .sensitive(false)
            .build();
        disp_row.add_prefix(disp_icon.widget());
        list_box.append(&disp_row);

        // Select Bluetooth by default
        if let Some(first_row) = list_box.row_at_index(0) {
            list_box.select_row(Some(&first_row));
        }

        let output_cb: OutputCallback = Rc::new(RefCell::new(None));

        // Connect row selection
        let cb = output_cb.clone();
        list_box.connect_row_selected(move |_, row| {
            if let Some(row) = row {
                let index = row.index();
                let category = match index {
                    0 => "Bluetooth",
                    1 => "Network",
                    2 => "Display",
                    _ => return,
                };
                if let Some(ref callback) = *cb.borrow() {
                    callback(SidebarOutput::Selected(category.to_string()));
                }
            }
        });

        Self {
            root: list_box,
            output_cb,
        }
    }

    /// Register a callback for sidebar output events.
    pub fn connect_output<F: Fn(SidebarOutput) + 'static>(&self, callback: F) {
        *self.output_cb.borrow_mut() = Some(Box::new(callback));
    }
}
