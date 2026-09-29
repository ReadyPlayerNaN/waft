//! Rename layout dialog -- simple alert dialog with a text entry.

use adw::prelude::*;
use std::rc::Rc;

use crate::i18n::t;

/// Show a dialog to rename a keyboard layout.
/// Calls `on_rename` with the new name if the user confirms.
pub fn show_rename_dialog(
    parent: &impl IsA<gtk::Widget>,
    current_name: &str,
    on_rename: impl Fn(String) + 'static,
) {
    let dialog = adw::AlertDialog::builder()
        .heading(t("kb-rename-dialog-heading"))
        .close_response("cancel")
        .build();

    dialog.add_response("cancel", &t("kb-rename-cancel"));
    dialog.add_response("rename", &t("kb-rename-confirm"));
    dialog.set_response_appearance("rename", adw::ResponseAppearance::Suggested);
    dialog.set_default_response(Some("rename"));

    let builder = gtk::Builder::from_resource("/com/waft/settings/rename-dialog.ui");
    let list_box: gtk::ListBox = builder
        .object("root")
        .expect("rename-dialog.ui must contain root");
    let entry: adw::EntryRow = builder
        .object("entry_row")
        .expect("rename-dialog.ui must contain entry_row");
    entry.set_title(&t("kb-rename-entry-title"));
    entry.set_text(current_name);
    entry.set_show_apply_button(false);

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
