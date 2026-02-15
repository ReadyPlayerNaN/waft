//! Settings category sidebar.
//!
//! Dumb widget displaying a list of settings categories. Emits
//! `SidebarOutput::Selected` when the user picks a category.
//! Supports dynamic visibility of rows (e.g. WiFi hidden when no adapter).

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
    wifi_row: adw::ActionRow,
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

        // WiFi row (hidden until adapter detected)
        let net_icon = IconWidget::from_name("network-wireless-symbolic", 16);
        let wifi_row = adw::ActionRow::builder()
            .title("WiFi")
            .activatable(true)
            .visible(false)
            .build();
        wifi_row.add_prefix(net_icon.widget());
        list_box.append(&wifi_row);

        // Wired row
        let wired_icon = IconWidget::from_name("network-wired-symbolic", 16);
        let wired_row = adw::ActionRow::builder()
            .title("Wired")
            .activatable(true)
            .build();
        wired_row.add_prefix(wired_icon.widget());
        list_box.append(&wired_row);

        // Weather row
        let weather_icon = IconWidget::from_name("weather-clear-symbolic", 16);
        let weather_row = adw::ActionRow::builder()
            .title("Weather")
            .activatable(true)
            .build();
        weather_row.add_prefix(weather_icon.widget());
        list_box.append(&weather_row);

        // Display row
        let disp_icon = IconWidget::from_name("preferences-desktop-display-symbolic", 16);
        let disp_row = adw::ActionRow::builder()
            .title("Display")
            .activatable(true)
            .build();
        disp_row.add_prefix(disp_icon.widget());
        list_box.append(&disp_row);

        // Keyboard row
        let kb_icon = IconWidget::from_name("input-keyboard-symbolic", 16);
        let kb_row = adw::ActionRow::builder()
            .title("Keyboard")
            .activatable(true)
            .build();
        kb_row.add_prefix(kb_icon.widget());
        list_box.append(&kb_row);

        // Notifications row
        let notif_icon =
            IconWidget::from_name("preferences-system-notifications-symbolic", 16);
        let notif_row = adw::ActionRow::builder()
            .title("Notifications")
            .activatable(true)
            .build();
        notif_row.add_prefix(notif_icon.widget());
        list_box.append(&notif_row);

        // Select Bluetooth by default
        if let Some(first_row) = list_box.row_at_index(0) {
            list_box.select_row(Some(&first_row));
        }

        let output_cb: OutputCallback = Rc::new(RefCell::new(None));

        // Connect row selection -- use row title instead of index
        // so hidden rows don't break the mapping.
        let cb = output_cb.clone();
        list_box.connect_row_selected(move |_, row| {
            if let Some(row) = row {
                // adw::ActionRow extends gtk::ListBoxRow, so downcast directly
                if let Some(action_row) = row.downcast_ref::<adw::ActionRow>() {
                    let title = action_row.title();
                    if let Some(ref callback) = *cb.borrow() {
                        callback(SidebarOutput::Selected(title.to_string()));
                    }
                }
            }
        });

        Self {
            root: list_box,
            output_cb,
            wifi_row,
        }
    }

    /// Show or hide the WiFi category row.
    ///
    /// If hiding and WiFi is currently selected, auto-selects Bluetooth.
    pub fn set_wifi_visible(&self, visible: bool) {
        self.wifi_row.set_visible(visible);

        // If hiding WiFi while it's selected, switch to Bluetooth
        if !visible
            && let Some(selected) = self.root.selected_row()
            && let Some(action_row) = selected.downcast_ref::<adw::ActionRow>()
            && action_row.title() == "WiFi"
            && let Some(bt_row) = self.root.row_at_index(0)
        {
            self.root.select_row(Some(&bt_row));
        }
    }

    /// Register a callback for sidebar output events.
    pub fn connect_output<F: Fn(SidebarOutput) + 'static>(&self, callback: F) {
        *self.output_cb.borrow_mut() = Some(Box::new(callback));
    }
}
