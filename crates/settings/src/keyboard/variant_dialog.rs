//! Variant selection dialog -- shows searchable list of available XKB variants for a layout.
//!
//! Uses adw::AlertDialog with a callback for when a variant is selected.

use adw::prelude::*;
use std::cell::RefCell;
use std::rc::Rc;

use crate::i18n::t;
use crate::keyboard::xkb_database;

/// Show the variant selection dialog for a specific layout.
///
/// Calls `on_select` with the selected variant code (empty string to clear variant).
pub fn show_variant_dialog(
    parent: &impl IsA<gtk::Widget>,
    layout_code: &str,
    current_variant: &str,
    on_select: impl Fn(String) + 'static,
) {
    let dialog = adw::AlertDialog::builder()
        .heading(t("kb-variant-dialog-heading"))
        .close_response("cancel")
        .build();

    dialog.add_response("cancel", &t("kb-variant-cancel"));
    dialog.add_response("select", &t("kb-variant-confirm"));
    dialog.set_response_appearance("select", adw::ResponseAppearance::Suggested);
    dialog.set_default_response(Some("select"));

    // Search entry
    let search_entry = gtk::SearchEntry::builder()
        .placeholder_text(t("kb-variant-search-placeholder"))
        .margin_bottom(8)
        .build();

    // Scrolled list
    let scrolled = gtk::ScrolledWindow::builder()
        .vexpand(true)
        .hexpand(true)
        .min_content_height(300)
        .min_content_width(350)
        .build();

    let list_box = gtk::ListBox::builder()
        .selection_mode(gtk::SelectionMode::Single)
        .css_classes(["boxed-list"])
        .build();
    scrolled.set_child(Some(&list_box));

    // "Default (no variant)" row
    let default_row = adw::ActionRow::builder()
        .title(t("kb-variant-none"))
        .subtitle("")
        .activatable(true)
        .build();
    list_box.append(&default_row);

    // Load variants for this layout
    let variants = xkb_database::get_variants_for_layout(layout_code);
    for variant in &variants {
        let row = adw::ActionRow::builder()
            .title(&variant.description)
            .subtitle(&variant.code)
            .activatable(true)
            .build();
        list_box.append(&row);
    }

    // Pre-select the current variant
    if current_variant.is_empty() {
        list_box.select_row(Some(&default_row));
    } else {
        // Find the row matching the current variant (index 0 is default, 1+ are variants)
        let selected_row = variants
            .iter()
            .enumerate()
            .find(|(_, v)| v.code == current_variant)
            .and_then(|(i, _)| list_box.row_at_index((i + 1) as i32));

        match selected_row {
            Some(row) => list_box.select_row(Some(&row)),
            None => list_box.select_row(Some(&default_row)),
        }
    }

    // Search filtering
    let search_entry_ref = search_entry.clone();
    list_box.set_filter_func(move |row| {
        let search_text = search_entry_ref.text().to_lowercase();
        if search_text.is_empty() {
            return true;
        }
        if let Some(action_row) = row.downcast_ref::<adw::ActionRow>() {
            let title = action_row.title().to_lowercase();
            let subtitle = action_row
                .subtitle()
                .map(|s| s.to_lowercase())
                .unwrap_or_default();
            title.contains(&search_text) || subtitle.contains(&search_text)
        } else {
            false
        }
    });

    let list_box_for_filter = list_box.clone();
    search_entry.connect_search_changed(move |_| {
        list_box_for_filter.invalidate_filter();
    });

    // Layout for dialog content
    let content_box = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .spacing(8)
        .build();
    content_box.append(&search_entry);
    content_box.append(&scrolled);

    dialog.set_extra_child(Some(&content_box));

    // Track selected variant code
    let selected: Rc<RefCell<Option<String>>> = Rc::new(RefCell::new(None));
    let selected_for_select = selected.clone();
    list_box.connect_row_selected(move |_, row| {
        if let Some(row) = row
            && let Some(action_row) = row.downcast_ref::<adw::ActionRow>()
        {
            let subtitle = action_row.subtitle().unwrap_or_default().to_string();
            // Empty subtitle means "default (no variant)"
            *selected_for_select.borrow_mut() = Some(subtitle);
        }
    });

    // Handle double-click to select and confirm
    let on_select = Rc::new(on_select);
    let on_select_for_activate = on_select.clone();
    let dialog_for_activate = dialog.clone();
    list_box.connect_row_activated(move |_, row| {
        if let Some(action_row) = row.downcast_ref::<adw::ActionRow>() {
            let subtitle = action_row.subtitle().unwrap_or_default().to_string();
            on_select_for_activate(subtitle);
            dialog_for_activate.force_close();
        }
    });

    let selected_for_response = selected;
    let on_select_for_response = on_select;
    dialog.connect_response(None, move |_, response| {
        if response == "select"
            && let Some(variant_code) = selected_for_response.borrow().clone()
        {
            on_select_for_response(variant_code);
        }
    });

    dialog.present(Some(parent));
}
