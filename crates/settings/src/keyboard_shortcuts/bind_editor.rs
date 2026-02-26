//! Dialog for adding or editing a keyboard shortcut.
//!
//! Provides modifier checkboxes, key name entry with validation, and action
//! selector with category grouping.

use std::cell::RefCell;
use std::rc::Rc;

use adw::prelude::*;

use crate::i18n::t;
use crate::keyboard_shortcuts::{all_action_names, validate_key, BindAction, BindEntry, Modifier};

type ConfirmedCallback = Rc<RefCell<Option<Box<dyn Fn(BindEntry)>>>>;

/// Dialog for creating or editing a keyboard shortcut.
pub struct BindEditor {
    dialog: adw::AlertDialog,
    on_confirmed: ConfirmedCallback,
}

impl BindEditor {
    /// Create a new bind editor dialog.
    ///
    /// If `initial` is provided, the fields are pre-populated for editing.
    pub fn new(initial: Option<&BindEntry>) -> Self {
        let dialog = adw::AlertDialog::builder()
            .heading(t("settings-keyboard-shortcuts"))
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

        // -- Modifiers --
        let mod_group = adw::PreferencesGroup::builder()
            .title(t("kb-modifier-mod"))
            .build();

        let mod_check = gtk::CheckButton::builder()
            .label(t("kb-modifier-mod"))
            .build();
        let shift_check = gtk::CheckButton::builder()
            .label(t("kb-modifier-shift"))
            .build();
        let ctrl_check = gtk::CheckButton::builder()
            .label(t("kb-modifier-ctrl"))
            .build();
        let alt_check = gtk::CheckButton::builder()
            .label(t("kb-modifier-alt"))
            .build();

        let mod_row = gtk::Box::builder()
            .orientation(gtk::Orientation::Horizontal)
            .spacing(12)
            .margin_start(12)
            .margin_end(12)
            .margin_top(6)
            .margin_bottom(6)
            .build();
        mod_row.append(&mod_check);
        mod_row.append(&shift_check);
        mod_row.append(&ctrl_check);
        mod_row.append(&alt_check);
        mod_group.add(&mod_row);
        content.append(&mod_group);

        // -- Key name entry --
        let key_group = adw::PreferencesGroup::new();
        let key_entry = adw::EntryRow::builder()
            .title(t("kb-key"))
            .build();
        key_group.add(&key_entry);
        content.append(&key_group);

        // -- Key validation label --
        let validation_label = gtk::Label::builder()
            .label("")
            .css_classes(["error"])
            .halign(gtk::Align::Start)
            .visible(false)
            .build();
        content.append(&validation_label);

        // -- Action selector --
        let action_group = adw::PreferencesGroup::new();
        let action_names = all_action_names();
        let action_list = gtk::StringList::new(&action_names.to_vec());
        let action_row = adw::ComboRow::builder()
            .title(t("kb-action"))
            .model(&action_list)
            .build();
        action_group.add(&action_row);
        content.append(&action_group);

        // -- Spawn command/args (shown when action is "spawn") --
        let spawn_group = adw::PreferencesGroup::new();
        let spawn_command_row = adw::EntryRow::builder()
            .title(t("startup-entry-command"))
            .build();
        let spawn_args_row = adw::EntryRow::builder()
            .title(t("startup-entry-args"))
            .build();
        spawn_group.add(&spawn_command_row);
        spawn_group.add(&spawn_args_row);
        content.append(&spawn_group);

        // -- Optional properties --
        let props_group = adw::PreferencesGroup::new();
        let title_row = adw::EntryRow::builder()
            .title(t("kb-hotkey-title"))
            .build();
        let locked_row = adw::SwitchRow::builder()
            .title(t("kb-allow-when-locked"))
            .build();
        props_group.add(&title_row);
        props_group.add(&locked_row);
        content.append(&props_group);

        dialog.set_extra_child(Some(&content));

        // Show/hide spawn fields based on action selection
        let spawn_group_ref = spawn_group.clone();
        action_row.connect_selected_notify(move |row| {
            let selected = row.selected();
            spawn_group_ref.set_visible(selected == 0); // "spawn" is index 0
        });

        // Pre-populate if editing
        if let Some(entry) = initial {
            mod_check.set_active(entry.modifiers.contains(&Modifier::Mod));
            shift_check.set_active(entry.modifiers.contains(&Modifier::Shift));
            ctrl_check.set_active(entry.modifiers.contains(&Modifier::Ctrl));
            alt_check.set_active(entry.modifiers.contains(&Modifier::Alt));
            key_entry.set_text(&entry.key);

            match &entry.action {
                BindAction::Spawn { command, args } => {
                    action_row.set_selected(0);
                    spawn_command_row.set_text(command);
                    spawn_args_row.set_text(&args.join(" "));
                    spawn_group.set_visible(true);
                }
                BindAction::NiriAction { name, .. } => {
                    if let Some(pos) = action_names.iter().position(|a| a == name) {
                        action_row.set_selected(pos as u32);
                    }
                    spawn_group.set_visible(false);
                }
            }

            if let Some(ref title) = entry.hotkey_overlay_title {
                title_row.set_text(title);
            }
            locked_row.set_active(entry.allow_when_locked);
        }

        // Validate key on change
        let validation_ref = validation_label.clone();
        let dialog_for_validate = dialog.clone();
        let key_for_validate = key_entry.clone();
        let validate = move || {
            let key_text = key_for_validate.text();
            let key_str = key_text.trim();
            if key_str.is_empty() {
                validation_ref.set_visible(false);
                dialog_for_validate.set_response_enabled("save", false);
            } else if validate_key(key_str) {
                validation_ref.set_visible(false);
                dialog_for_validate.set_response_enabled("save", true);
            } else {
                validation_ref.set_label(&format!("Unknown key: {key_str}"));
                validation_ref.set_visible(true);
                dialog_for_validate.set_response_enabled("save", false);
            }
        };
        let validate_fn = validate.clone();
        key_entry.connect_changed(move |_| validate_fn());
        validate();

        let on_confirmed: ConfirmedCallback = Rc::new(RefCell::new(None));

        // Wire response
        let cb = on_confirmed.clone();
        dialog.connect_response(None, move |_, response| {
            if response != "save" {
                return;
            }

            let key_text = key_entry.text().trim().to_string();
            if key_text.is_empty() || !validate_key(&key_text) {
                return;
            }

            let mut modifiers = Vec::new();
            if mod_check.is_active() {
                modifiers.push(Modifier::Mod);
            }
            if shift_check.is_active() {
                modifiers.push(Modifier::Shift);
            }
            if ctrl_check.is_active() {
                modifiers.push(Modifier::Ctrl);
            }
            if alt_check.is_active() {
                modifiers.push(Modifier::Alt);
            }

            let selected_idx = action_row.selected() as usize;
            let action = if selected_idx == 0 {
                // Spawn
                let command = spawn_command_row.text().trim().to_string();
                if command.is_empty() {
                    return;
                }
                let args_text = spawn_args_row.text().trim().to_string();
                let args = if args_text.is_empty() {
                    Vec::new()
                } else {
                    crate::startup::entry_dialog::shell_words_split(&args_text)
                };
                BindAction::Spawn { command, args }
            } else {
                let name = action_names[selected_idx].to_string();
                BindAction::NiriAction {
                    name,
                    args: Vec::new(),
                }
            };

            let hotkey_overlay_title = {
                let text = title_row.text().trim().to_string();
                if text.is_empty() { None } else { Some(text) }
            };

            let entry = BindEntry {
                modifiers,
                key: key_text,
                action,
                hotkey_overlay_title,
                allow_when_locked: locked_row.is_active(),
                repeat: None,
            };

            if let Some(ref callback) = *cb.borrow() {
                callback(entry);
            }
        });

        Self {
            dialog,
            on_confirmed,
        }
    }

    /// Register a callback for when the user confirms the dialog.
    pub fn connect_confirmed(&self, cb: impl Fn(BindEntry) + 'static) {
        *self.on_confirmed.borrow_mut() = Some(Box::new(cb));
    }

    /// Present the dialog on the given widget.
    pub fn present(&self, parent: &impl IsA<gtk::Widget>) {
        self.dialog.present(Some(parent));
    }
}
