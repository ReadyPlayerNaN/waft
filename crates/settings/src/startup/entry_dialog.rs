//! Dialog for adding or editing a startup entry.
//!
//! Provides command and arguments fields with save/cancel buttons.

use std::cell::RefCell;
use std::rc::Rc;

use adw::prelude::*;

use crate::i18n::t;
use crate::startup::StartupEntry;

type ConfirmedCallback = Rc<RefCell<Option<Box<dyn Fn(StartupEntry)>>>>;

/// Dialog for creating or editing a startup entry.
pub struct EntryDialog {
    dialog: adw::AlertDialog,
    on_confirmed: ConfirmedCallback,
}

impl EntryDialog {
    /// Create a new entry dialog.
    ///
    /// If `initial` is provided, the fields are pre-populated for editing.
    pub fn new(initial: Option<&StartupEntry>) -> Self {
        let dialog = adw::AlertDialog::builder()
            .heading(t("startup-entries"))
            .close_response("cancel")
            .default_response("save")
            .build();

        dialog.add_response("cancel", &t("notif-cancel"));
        dialog.add_response("save", &t("startup-save"));
        dialog.set_response_appearance("save", adw::ResponseAppearance::Suggested);

        let content = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .spacing(12)
            .margin_top(12)
            .margin_bottom(12)
            .build();

        let group = adw::PreferencesGroup::new();

        let command_row = adw::EntryRow::builder()
            .title(t("startup-entry-command"))
            .build();

        let args_row = adw::EntryRow::builder()
            .title(t("startup-entry-args"))
            .build();

        if let Some(entry) = initial {
            command_row.set_text(&entry.command);
            args_row.set_text(&entry.args.join(" "));
        }

        group.add(&command_row);
        group.add(&args_row);
        content.append(&group);

        dialog.set_extra_child(Some(&content));

        let on_confirmed: ConfirmedCallback = Rc::new(RefCell::new(None));

        // Disable save when command is empty
        let cmd_for_validate = command_row.clone();
        let dialog_for_validate = dialog.clone();
        let update_sensitivity = move || {
            let has_command = !cmd_for_validate.text().trim().is_empty();
            dialog_for_validate.set_response_enabled("save", has_command);
        };
        let update_fn = update_sensitivity.clone();
        command_row.connect_changed(move |_| update_fn());
        update_sensitivity();

        // Wire response
        let cmd_for_response = command_row.clone();
        let args_for_response = args_row.clone();
        let cb = on_confirmed.clone();
        dialog.connect_response(None, move |_, response| {
            if response == "save" {
                let command = cmd_for_response.text().trim().to_string();
                if command.is_empty() {
                    return;
                }
                let args_text = args_for_response.text().trim().to_string();
                let args: Vec<String> = if args_text.is_empty() {
                    Vec::new()
                } else {
                    shell_words_split(&args_text)
                };
                let entry = StartupEntry { command, args };
                if let Some(ref callback) = *cb.borrow() {
                    callback(entry);
                }
            }
        });

        Self {
            dialog,
            on_confirmed,
        }
    }

    /// Register a callback for when the user confirms the dialog.
    pub fn connect_confirmed(&self, cb: impl Fn(StartupEntry) + 'static) {
        *self.on_confirmed.borrow_mut() = Some(Box::new(cb));
    }

    /// Present the dialog on the given widget.
    pub fn present(&self, parent: &impl IsA<gtk::Widget>) {
        self.dialog.present(Some(parent));
    }
}

/// Simple shell-like word splitting (handles quoted strings).
pub fn shell_words_split(s: &str) -> Vec<String> {
    let mut words = Vec::new();
    let mut current = String::new();
    let mut in_single_quote = false;
    let mut in_double_quote = false;
    let mut escape_next = false;

    for ch in s.chars() {
        if escape_next {
            current.push(ch);
            escape_next = false;
            continue;
        }
        match ch {
            '\\' if !in_single_quote => {
                escape_next = true;
            }
            '\'' if !in_double_quote => {
                in_single_quote = !in_single_quote;
            }
            '"' if !in_single_quote => {
                in_double_quote = !in_double_quote;
            }
            ' ' | '\t' if !in_single_quote && !in_double_quote => {
                if !current.is_empty() {
                    words.push(std::mem::take(&mut current));
                }
            }
            _ => {
                current.push(ch);
            }
        }
    }
    if !current.is_empty() {
        words.push(current);
    }
    words
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn split_simple_args() {
        assert_eq!(shell_words_split("arg1 arg2 arg3"), vec!["arg1", "arg2", "arg3"]);
    }

    #[test]
    fn split_quoted_args() {
        assert_eq!(shell_words_split(r#"-c "echo hello""#), vec!["-c", "echo hello"]);
    }

    #[test]
    fn split_empty() {
        assert_eq!(shell_words_split(""), Vec::<String>::new());
    }
}
