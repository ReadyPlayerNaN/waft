//! Reusable empty-state + preferences-group + list-box scaffolding.
//!
//! Used by smart container pages that display a single list of entities
//! with an empty state placeholder (plugins, services, scheduler, online accounts).

use adw::prelude::*;

/// A group containing an empty state placeholder, a preferences group, and a
/// list box. Provides helpers for sorted insertion and stale-row removal.
pub struct EntityListGroup {
    pub empty_state: adw::StatusPage,
    pub group: adw::PreferencesGroup,
    pub list_box: gtk::ListBox,
}

impl EntityListGroup {
    /// Build empty_state + PreferencesGroup + ListBox and append both to `parent`.
    pub fn new(
        parent: &gtk::Box,
        icon_name: &str,
        empty_title: &str,
        empty_description: &str,
        group_title: &str,
    ) -> Self {
        let empty_state = adw::StatusPage::builder()
            .icon_name(icon_name)
            .title(empty_title)
            .description(empty_description)
            .visible(false)
            .build();
        parent.append(&empty_state);

        let group = adw::PreferencesGroup::builder()
            .title(group_title)
            .visible(false)
            .build();

        let list_box = gtk::ListBox::builder()
            .selection_mode(gtk::SelectionMode::None)
            .css_classes(["boxed-list"])
            .build();
        group.add(&list_box);
        parent.append(&group);

        Self {
            empty_state,
            group,
            list_box,
        }
    }

    /// Insert `widget` into the list box at the position matching `key` in
    /// the pre-sorted `sorted_keys` slice.
    pub fn insert_sorted(&self, widget: &impl IsA<gtk::Widget>, key: &str, sorted_keys: &[String]) {
        let pos = sorted_keys
            .iter()
            .position(|k| k == key)
            .unwrap_or(0);
        self.list_box.insert(widget, pos as i32);
    }

    /// Toggle visibility: show the group when `has_items` is true, otherwise
    /// show the empty state.
    pub fn toggle_visibility(&self, has_items: bool) {
        self.group.set_visible(has_items);
        self.empty_state.set_visible(!has_items);
    }
}
