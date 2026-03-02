//! WiFi password dialog -- alert dialog with a password entry.

use adw::prelude::*;
use std::rc::Rc;

use crate::i18n::{t, t_args};

/// Show a password prompt dialog for a WiFi network.
///
/// Calls `on_connect` with the entered password when the user clicks Connect.
pub fn show_password_dialog(
    parent: &impl IsA<gtk::Widget>,
    ssid: &str,
    on_connect: impl Fn(String) + 'static,
) {
    let dialog = adw::AlertDialog::builder()
        .heading(t_args("wifi-password-title", &[("ssid", ssid)]))
        .body(t("wifi-password-body"))
        .close_response("cancel")
        .build();

    dialog.add_response("cancel", &t("wifi-password-cancel"));
    dialog.add_response("connect", &t("wifi-password-connect"));
    dialog.set_response_appearance("connect", adw::ResponseAppearance::Suggested);
    dialog.set_default_response(Some("connect"));

    let entry = gtk::PasswordEntry::builder()
        .show_peek_icon(true)
        .build();

    dialog.set_extra_child(Some(&entry));

    // Disable Connect button when password is empty
    dialog.set_response_enabled("connect", false);
    let dialog_ref = dialog.clone();
    entry.connect_changed(move |e| {
        dialog_ref.set_response_enabled("connect", !e.text().is_empty());
    });

    let on_connect = Rc::new(on_connect);

    // Enter key in entry confirms
    let dialog_for_activate = dialog.clone();
    let entry_for_activate = entry.clone();
    let on_connect_for_activate = on_connect.clone();
    entry.connect_activate(move |_| {
        let pw = entry_for_activate.text().to_string();
        if !pw.is_empty() {
            on_connect_for_activate(pw);
            dialog_for_activate.force_close();
        }
    });

    let entry_for_response = entry.clone();
    dialog.connect_response(None, move |_, response| {
        if response == "connect" {
            let pw = entry_for_response.text().to_string();
            if !pw.is_empty() {
                on_connect(pw);
            }
        }
    });

    dialog.present(Some(parent));
}
