//! Settings category sidebar.
//!
//! Dumb widget displaying settings items grouped by category. Emits
//! `SidebarOutput::Selected` when the user picks a page.
//! Supports dynamic visibility of rows (e.g. WiFi hidden when no adapter).
//! Includes a search bar that filters settings and shows results inline.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use adw::prelude::*;
use gtk::glib;
use waft_ui_gtk::icons::IconWidget;

use crate::i18n::t;
use crate::search_index::SearchIndex;
use crate::search_results::{SearchResultRef, SearchResults, SearchResultsOutput};

/// Output events from the sidebar.
pub enum SidebarOutput {
    /// A page was selected by the user (from category list or search).
    Selected {
        /// Stable identifier for stack page routing.
        page_id: String,
        /// Human-readable title for the content header.
        title: String,
    },
    /// A search result was selected with a target widget to scroll to.
    SearchSelected {
        /// Stable identifier for stack page routing.
        page_id: String,
        /// Human-readable title for the content header.
        page_title: String,
        /// Target widget to scroll to.
        target_widget: Option<glib::WeakRef<gtk::Widget>>,
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
                SidebarItem {
                    page_id: "online-accounts",
                    title: t("settings-online-accounts"),
                    icon: "contacts-symbolic",
                    visible: true,
                },
            ],
        },
        SidebarCategory {
            label: t("sidebar-visual"),
            items: vec![
                SidebarItem {
                    page_id: "appearance",
                    title: t("settings-appearance"),
                    icon: "preferences-desktop-appearance-symbolic",
                    visible: true,
                },
                SidebarItem {
                    page_id: "display",
                    title: t("settings-display"),
                    icon: "preferences-desktop-display-symbolic",
                    visible: true,
                },
                SidebarItem {
                    page_id: "windows",
                    title: t("settings-windows"),
                    icon: "preferences-desktop-default-applications-symbolic",
                    visible: true,
                },
                SidebarItem {
                    page_id: "wallpaper",
                    title: t("settings-wallpaper"),
                    icon: "preferences-desktop-wallpaper-symbolic",
                    visible: true,
                },
            ],
        },
        SidebarCategory {
            label: t("sidebar-feedback"),
            items: vec![
                SidebarItem {
                    page_id: "audio",
                    title: t("settings-audio"),
                    icon: "audio-volume-high-symbolic",
                    visible: true,
                },
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
            items: vec![
                SidebarItem {
                    page_id: "keyboard",
                    title: t("settings-keyboard"),
                    icon: "input-keyboard-symbolic",
                    visible: true,
                },
                SidebarItem {
                    page_id: "keyboard-shortcuts",
                    title: t("settings-keyboard-shortcuts"),
                    icon: "preferences-desktop-keyboard-shortcuts-symbolic",
                    visible: true,
                },
            ],
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
            items: vec![
                SidebarItem {
                    page_id: "plugins",
                    title: t("settings-plugins"),
                    icon: "application-x-addon-symbolic",
                    visible: true,
                },
                SidebarItem {
                    page_id: "services",
                    title: t("settings-services"),
                    icon: "system-run-symbolic",
                    visible: true,
                },
                SidebarItem {
                    page_id: "startup",
                    title: t("settings-startup"),
                    icon: "system-run-symbolic",
                    visible: true,
                },
            ],
        },
        SidebarCategory {
            label: t("sidebar-automation"),
            items: vec![
                SidebarItem {
                    page_id: "scheduled-tasks",
                    title: t("settings-scheduled-tasks"),
                    icon: "preferences-system-time-symbolic",
                    visible: true,
                },
            ],
        },
    ]
}

/// Category sidebar widget with integrated search.
pub struct Sidebar {
    pub root: gtk::Box,
    pub search_bar: gtk::SearchBar,
    output_cb: OutputCallback,
    wifi_row: adw::ActionRow,
    list_boxes: Vec<gtk::ListBox>,
    search_index: Rc<RefCell<SearchIndex>>,
}

impl Sidebar {
    pub fn new(search_index: Rc<RefCell<SearchIndex>>) -> Self {
        let container = gtk::Box::new(gtk::Orientation::Vertical, 0);
        let output_cb: OutputCallback = Rc::new(RefCell::new(None));
        let selecting = Rc::new(Cell::new(false));

        // -- Search bar --
        let search_entry = gtk::SearchEntry::builder()
            .placeholder_text(t("search-placeholder"))
            .hexpand(true)
            .build();

        let search_bar = gtk::SearchBar::builder()
            .child(&search_entry)
            .show_close_button(false)
            .build();
        search_bar.connect_entry(&search_entry);
        container.append(&search_bar);

        // -- Categories container (shown when not searching) --
        let categories_box = gtk::Box::new(gtk::Orientation::Vertical, 0);
        container.append(&categories_box);

        // -- Search results (shown when searching) --
        let search_results = SearchResults::new();
        search_results.root.set_visible(false);
        container.append(&search_results.root);

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
            categories_box.append(&label);

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

            categories_box.append(&list_box);
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

        // -- Wire search --
        {
            let index = search_index.clone();
            let results_widget = search_results.root.clone();
            let categories_ref = categories_box.clone();
            let results_ref = Rc::new(search_results);

            // Wire search entry text changes
            let results_for_search = results_ref.clone();
            search_entry.connect_search_changed(move |entry| {
                let query = entry.text().to_string();
                if query.trim().is_empty() {
                    categories_ref.set_visible(true);
                    results_widget.set_visible(false);
                    results_for_search.clear();
                } else {
                    categories_ref.set_visible(false);
                    results_widget.set_visible(true);

                    let idx = index.borrow();
                    let matches = idx.search(&query);
                    let refs: Vec<SearchResultRef> = matches
                        .iter()
                        .map(|e| SearchResultRef {
                            page_id: e.page_id,
                            page_title: e.page_title.clone(),
                            breadcrumb: e.breadcrumb(),
                        })
                        .collect();
                    results_for_search.update(&refs);
                }
            });

            // Wire search result selection
            let cb_for_results = output_cb.clone();
            let search_bar_ref = search_bar.clone();
            let search_entry_ref = search_entry.clone();
            let index_for_select = search_index.clone();
            results_ref.connect_output(move |output| {
                let SearchResultsOutput::Selected {
                    page_id, page_title, ..
                } = output;

                // Look up the target_widget from the index for this result
                let target_widget = {
                    let idx = index_for_select.borrow();
                    let results = idx.search(&search_entry_ref.text());
                    results
                        .iter()
                        .find(|e| e.page_id == page_id)
                        .and_then(|e| e.target_widget.clone())
                };

                // Clear search and dismiss
                search_entry_ref.set_text("");
                search_bar_ref.set_search_mode(false);

                if let Some(ref callback) = *cb_for_results.borrow() {
                    callback(SidebarOutput::SearchSelected {
                        page_id,
                        page_title,
                        target_widget,
                    });
                }
            });

            // Keep results_ref alive
            std::mem::forget(results_ref);
        }

        let wifi_row = wifi_row_slot.expect("WiFi row must exist in category definitions");

        Self {
            root: container,
            search_bar,
            output_cb,
            wifi_row,
            list_boxes,
            search_index,
        }
    }

    /// Show or hide the WiFi category row.
    ///
    /// If hiding and WiFi is currently selected, auto-selects Bluetooth.
    pub fn set_wifi_visible(&self, visible: bool) {
        self.wifi_row.set_visible(visible);
        self.search_index.borrow_mut().set_page_visible("wifi", visible);

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

    /// Programmatically navigate to the sidebar row for the given page ID.
    ///
    /// Must be called AFTER `connect_output` is registered so that the output
    /// callback fires and both the stack and the content header title update.
    /// If the row is already selected (e.g. "bluetooth" on first launch), GTK
    /// will not re-emit `row-selected`, making this effectively a no-op.
    pub fn navigate_to(&self, page_id: &str) {
        for list_box in &self.list_boxes {
            let mut idx = 0;
            while let Some(row) = list_box.row_at_index(idx) {
                if let Some(action_row) = row.downcast_ref::<adw::ActionRow>()
                    && action_row.widget_name() == page_id
                {
                    list_box.select_row(Some(&row));
                    return;
                }
                idx += 1;
            }
        }
    }

    /// Register a callback for sidebar output events.
    pub fn connect_output<F: Fn(SidebarOutput) + 'static>(&self, callback: F) {
        *self.output_cb.borrow_mut() = Some(Box::new(callback));
    }
}
