//! Add layout dialog -- shows searchable list of available XKB layouts.
//!
//! Uses adw::AlertDialog with a callback for when a layout is selected.

use adw::prelude::*;
use std::cell::RefCell;
use std::rc::Rc;

/// Available XKB layout entry.
struct AvailableLayout {
    code: String,
    name: String,
}

/// Show the add layout dialog. Calls `on_add` with the selected layout code
/// if the user confirms selection.
pub fn show_add_layout_dialog(parent: &impl IsA<gtk::Widget>, on_add: impl Fn(String) + 'static) {
    let dialog = adw::AlertDialog::builder()
        .heading("Add Keyboard Layout")
        .close_response("cancel")
        .build();

    dialog.add_response("cancel", "Cancel");
    dialog.add_response("add", "Add");
    dialog.set_response_appearance("add", adw::ResponseAppearance::Suggested);
    dialog.set_default_response(Some("add"));

    // Search entry
    let search_entry = gtk::SearchEntry::builder()
        .placeholder_text("Search layouts...")
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

    // Load available layouts
    let available = get_available_layouts();
    for layout in &available {
        let row = adw::ActionRow::builder()
            .title(&layout.name)
            .subtitle(&layout.code)
            .activatable(true)
            .build();
        list_box.append(&row);
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

    // Track selected row
    let selected_code: Rc<RefCell<Option<String>>> = Rc::new(RefCell::new(None));
    let selected_code_for_select = selected_code.clone();
    list_box.connect_row_selected(move |_, row| {
        if let Some(row) = row {
            if let Some(action_row) = row.downcast_ref::<adw::ActionRow>() {
                *selected_code_for_select.borrow_mut() =
                    action_row.subtitle().map(|s| s.to_string());
            }
        }
    });

    // Handle double-click to select and confirm
    let on_add = Rc::new(on_add);
    let on_add_for_activate = on_add.clone();
    let dialog_for_activate = dialog.clone();
    list_box.connect_row_activated(move |_, row| {
        if let Some(action_row) = row.downcast_ref::<adw::ActionRow>() {
            if let Some(code) = action_row.subtitle() {
                on_add_for_activate(code.to_string());
                dialog_for_activate.force_close();
            }
        }
    });

    let selected_code_for_response = selected_code;
    let on_add_for_response = on_add;
    dialog.connect_response(None, move |_, response| {
        if response == "add" {
            if let Some(code) = selected_code_for_response.borrow().clone() {
                on_add_for_response(code);
            }
        }
    });

    dialog.present(Some(parent));
}

fn get_available_layouts() -> Vec<AvailableLayout> {
    // Try parsing the system XKB database first
    match parse_xkb_layouts() {
        Ok(layouts) if !layouts.is_empty() => layouts,
        Ok(_) | Err(_) => {
            log::debug!("[keyboard] Using fallback layout list");
            get_fallback_layouts()
        }
    }
}

/// Parse available layouts from /usr/share/X11/xkb/rules/base.lst.
fn parse_xkb_layouts() -> Result<Vec<AvailableLayout>, std::io::Error> {
    let content = std::fs::read_to_string("/usr/share/X11/xkb/rules/base.lst")?;
    let mut layouts = Vec::new();
    let mut in_layout_section = false;

    for line in content.lines() {
        let trimmed = line.trim();

        if trimmed == "! layout" {
            in_layout_section = true;
            continue;
        }

        if trimmed.starts_with('!') {
            if in_layout_section {
                break;
            }
            continue;
        }

        if in_layout_section && !trimmed.is_empty() {
            // Format: "  code           Description"
            let mut parts = trimmed.splitn(2, char::is_whitespace);
            if let (Some(code), Some(rest)) = (parts.next(), parts.next()) {
                let name = rest.trim();
                layouts.push(AvailableLayout {
                    code: code.to_string(),
                    name: name.to_string(),
                });
            }
        }
    }

    Ok(layouts)
}

fn get_fallback_layouts() -> Vec<AvailableLayout> {
    [
        ("us", "English (US)"),
        ("gb", "English (UK)"),
        ("de", "German"),
        ("fr", "French"),
        ("es", "Spanish"),
        ("it", "Italian"),
        ("pt", "Portuguese"),
        ("nl", "Dutch"),
        ("se", "Swedish"),
        ("no", "Norwegian"),
        ("dk", "Danish"),
        ("fi", "Finnish"),
        ("pl", "Polish"),
        ("cz", "Czech"),
        ("sk", "Slovak"),
        ("hu", "Hungarian"),
        ("ro", "Romanian"),
        ("bg", "Bulgarian"),
        ("hr", "Croatian"),
        ("rs", "Serbian"),
        ("si", "Slovenian"),
        ("ru", "Russian"),
        ("ua", "Ukrainian"),
        ("gr", "Greek"),
        ("tr", "Turkish"),
        ("il", "Hebrew"),
        ("ara", "Arabic"),
        ("jp", "Japanese"),
        ("kr", "Korean"),
        ("cn", "Chinese"),
    ]
    .into_iter()
    .map(|(code, name)| AvailableLayout {
        code: code.to_string(),
        name: name.to_string(),
    })
    .collect()
}
