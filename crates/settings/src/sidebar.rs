//! Settings category sidebar.
//!
//! Dumb widget displaying settings items grouped by category. Emits
//! `SidebarOutput::Selected` when the user picks a page.
//! Supports dynamic visibility of rows (e.g. WiFi hidden when no adapter).

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use adw::prelude::*;
use waft_ui_gtk::widgets::icon::IconWidget;

use crate::i18n::t;

/// Output events from the sidebar.
pub enum SidebarOutput {
    /// A page was selected by the user.
    Selected {
        /// Stable identifier for stack page routing.
        page_id: String,
        /// Human-readable title for the content header.
        title: String,
    },
}

/// Callback type for sidebar output events.
type OutputCallback = Rc<RefCell<Option<Box<dyn Fn(SidebarOutput)>>>>;

/// An item in the sidebar.
struct SidebarItem {
    /// Stable identifier used for stack page routing.
    page_id: &'static str,
    /// Display title shown to the user.
    title: String,
    /// Icon name.
    icon: &'static str,
    /// Initial visibility.
    visible: bool,
}

/// A group of sidebar items under a category header.
struct SidebarCategory {
    label: String,
    items: Vec<SidebarItem>,
}

/// Returns the category layout for the sidebar.
fn categories() -> Vec<SidebarCategory> {
    vec![
        SidebarCategory {
            label: t("sidebar-connectivity"),
            items: vec![
                SidebarItem {
                    page_id: "bluetooth",
                    title: t("settings-bluetooth"),
                    icon: "bluetooth-active-symbolic",
                    visible: true,
                },
                SidebarItem {
                    page_id: "wifi",
                    title: t("settings-wifi"),
                    icon: "network-wireless-symbolic",
                    visible: false,
                },
                SidebarItem {
                    page_id: "wired",
                    title: t("settings-wired"),
                    icon: "network-wired-symbolic",
                    visible: true,
                },
            ],
        },
        SidebarCategory {
            label: t("sidebar-visual"),
            items: vec![SidebarItem {
                page_id: "display",
                title: t("settings-display"),
                icon: "preferences-desktop-display-symbolic",
                visible: true,
            }],
        },
        SidebarCategory {
            label: t("sidebar-feedback"),
            items: vec![
                SidebarItem {
                    page_id: "notifications",
                    title: t("settings-notifications"),
                    icon: "preferences-system-notifications-symbolic",
                    visible: true,
                },
                SidebarItem {
                    page_id: "sounds",
                    title: t("settings-sounds"),
                    icon: "audio-speakers-symbolic",
                    visible: true,
                },
            ],
        },
        SidebarCategory {
            label: t("sidebar-inputs"),
            items: vec![SidebarItem {
                page_id: "keyboard",
                title: t("settings-keyboard"),
                icon: "input-keyboard-symbolic",
                visible: true,
            }],
        },
        SidebarCategory {
            label: t("sidebar-info"),
            items: vec![SidebarItem {
                page_id: "weather",
                title: t("settings-weather"),
                icon: "weather-clear-symbolic",
                visible: true,
            }],
        },
        SidebarCategory {
            label: t("sidebar-system"),
            items: vec![SidebarItem {
                page_id: "plugins",
                title: t("settings-plugins"),
                icon: "application-x-addon-symbolic",
                visible: true,
            }],
        },
    ]
}

/// Category sidebar widget.
pub struct Sidebar {
    pub root: gtk::Box,
    output_cb: OutputCallback,
    wifi_row: adw::ActionRow,
    list_boxes: Vec<gtk::ListBox>,
}

impl Sidebar {
    pub fn new() -> Self {
        let container = gtk::Box::new(gtk::Orientation::Vertical, 0);
        let output_cb: OutputCallback = Rc::new(RefCell::new(None));
        let selecting = Rc::new(Cell::new(false));

        let mut list_boxes: Vec<gtk::ListBox> = Vec::new();
        let mut wifi_row_slot: Option<adw::ActionRow> = None;

        for (cat_idx, category) in categories().into_iter().enumerate() {
            // Category header label
            let label = gtk::Label::builder()
                .label(&category.label)
                .css_classes(["heading"])
                .halign(gtk::Align::Start)
                .margin_start(12)
                .margin_bottom(4)
                .build();
            if cat_idx > 0 {
                label.set_margin_top(12);
            }
            container.append(&label);

            // ListBox for this category's items
            let list_box = gtk::ListBox::builder()
                .selection_mode(gtk::SelectionMode::Single)
                .css_classes(["navigation-sidebar"])
                .build();

            for item in &category.items {
                let icon = IconWidget::from_name(item.icon, 16);
                let row = adw::ActionRow::builder()
                    .title(&item.title)
                    .activatable(true)
                    .visible(item.visible)
                    .build();
                row.add_prefix(icon.widget());

                // Store page_id on the row as widget name for retrieval in selection handler
                row.set_widget_name(item.page_id);

                if item.page_id == "wifi" {
                    wifi_row_slot = Some(row.clone());
                }

                list_box.append(&row);
            }

            container.append(&list_box);
            list_boxes.push(list_box);
        }

        // Wire up cross-group selection for each ListBox
        for (i, list_box) in list_boxes.iter().enumerate() {
            let all_boxes = list_boxes.clone();
            let selecting = selecting.clone();
            let cb = output_cb.clone();

            list_box.connect_row_selected(move |_, row| {
                if selecting.get() {
                    return;
                }
                if let Some(row) = row {
                    selecting.set(true);
                    for (j, other) in all_boxes.iter().enumerate() {
                        if i != j {
                            other.select_row(gtk::ListBoxRow::NONE);
                        }
                    }
                    selecting.set(false);

                    if let Some(action_row) = row.downcast_ref::<adw::ActionRow>() {
                        let page_id = action_row.widget_name().to_string();
                        let title = action_row.title().to_string();
                        if let Some(ref callback) = *cb.borrow() {
                            callback(SidebarOutput::Selected { page_id, title });
                        }
                    }
                }
            });
        }

        // Select Bluetooth (first row of first category) by default
        if let Some(first_box) = list_boxes.first()
            && let Some(first_row) = first_box.row_at_index(0)
        {
            first_box.select_row(Some(&first_row));
        }

        let wifi_row = wifi_row_slot.expect("WiFi row must exist in category definitions");

        Self {
            root: container,
            output_cb,
            wifi_row,
            list_boxes,
        }
    }

    /// Show or hide the WiFi category row.
    ///
    /// If hiding and WiFi is currently selected, auto-selects Bluetooth.
    pub fn set_wifi_visible(&self, visible: bool) {
        self.wifi_row.set_visible(visible);

        if !visible
            && let Some(connectivity_box) = self.list_boxes.first()
            && let Some(selected) = connectivity_box.selected_row()
            && let Some(action_row) = selected.downcast_ref::<adw::ActionRow>()
            && action_row.widget_name() == "wifi"
            && let Some(bt_row) = connectivity_box.row_at_index(0)
        {
            connectivity_box.select_row(Some(&bt_row));
        }
    }

    /// Register a callback for sidebar output events.
    pub fn connect_output<F: Fn(SidebarOutput) + 'static>(&self, callback: F) {
        *self.output_cb.borrow_mut() = Some(Box::new(callback));
    }
}
