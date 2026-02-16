//! Rename layout dialog -- simple alert dialog with a text entry.

use adw::prelude::*;
use std::rc::Rc;

/// Show a dialog to rename a keyboard layout.
/// Calls `on_rename` with the new name if the user confirms.
pub fn show_rename_dialog(
    parent: &impl IsA<gtk::Widget>,
    current_name: &str,
    on_rename: impl Fn(String) + 'static,
) {
    let dialog = adw::AlertDialog::builder()
        .heading("Rename Layout")
        .close_response("cancel")
        .build();

    dialog.add_response("cancel", "Cancel");
    dialog.add_response("rename", "Rename");
    dialog.set_response_appearance("rename", adw::ResponseAppearance::Suggested);
    dialog.set_default_response(Some("rename"));

    let entry = adw::EntryRow::builder()
        .title("Layout name")
        .text(current_name)
        .show_apply_button(false)
        .build();

    let list_box = gtk::ListBox::builder()
        .selection_mode(gtk::SelectionMode::None)
        .css_classes(["boxed-list"])
        .build();
    list_box.append(&entry);

    dialog.set_extra_child(Some(&list_box));

    let on_rename = Rc::new(on_rename);
    let entry_for_response = entry.clone();
    let on_rename_for_response = on_rename.clone();

    // Enter key in entry confirms
    let dialog_for_entry = dialog.clone();
    let on_rename_for_entry = on_rename;
    entry.connect_apply(move |entry| {
        let name = entry.text().to_string();
        if !name.is_empty() {
            on_rename_for_entry(name);
            dialog_for_entry.force_close();
        }
    });

    dialog.connect_response(None, move |_, response| {
        if response == "rename" {
            let name = entry_for_response.text().to_string();
            if !name.is_empty() {
                on_rename_for_response(name);
            }
        }
    });

    dialog.present(Some(parent));
}
