//! Keyboard Shortcuts settings page -- smart container.
//!
//! Reads niri `binds { }` entries from `~/.config/niri/config.kdl`
//! and allows adding, editing, and removing keyboard shortcuts.

use std::cell::RefCell;
use std::rc::Rc;

use adw::prelude::*;
use waft_ui_gtk::vdom::Component;

use crate::i18n::t;
use crate::keyboard_shortcuts::bind_editor::BindEditor;
use crate::keyboard_shortcuts::bind_row::{BindRow, BindRowOutput, BindRowProps};
use crate::keyboard_shortcuts::{self, BindEntry};
use crate::search_index::SearchIndex;
use crate::startup; // reuse niri_config_path()

/// Smart container for the Keyboard Shortcuts settings page.
pub struct KeyboardShortcutsPage {
    pub root: gtk::Box,
}

/// Internal mutable state.
struct ShortcutsPageState {
    entries: Vec<BindEntry>,
    raw_nodes: Vec<String>,
    rows: Vec<BindRow>,
    list_box: gtk::ListBox,
    raw_list_box: gtk::ListBox,
    empty_state: adw::StatusPage,
    group: adw::PreferencesGroup,
    raw_group: adw::PreferencesGroup,
    error_state: adw::StatusPage,
}

impl KeyboardShortcutsPage {
    pub fn new(search_index: &Rc<RefCell<SearchIndex>>) -> Self {
        let root = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .spacing(24)
            .margin_top(24)
            .margin_bottom(24)
            .margin_start(12)
            .margin_end(12)
            .build();

        let error_state = adw::StatusPage::builder()
            .icon_name("dialog-error-symbolic")
            .title(t("kb-shortcuts-parse-error"))
            .visible(false)
            .build();
        root.append(&error_state);

        let empty_state = adw::StatusPage::builder()
            .icon_name("preferences-desktop-keyboard-shortcuts-symbolic")
            .title(t("kb-shortcuts-no-entries"))
            .visible(false)
            .build();
        root.append(&empty_state);

        let group = adw::PreferencesGroup::builder()
            .title(t("kb-shortcuts-custom"))
            .visible(false)
            .build();
        let list_box = gtk::ListBox::builder()
            .selection_mode(gtk::SelectionMode::None)
            .css_classes(["boxed-list"])
            .build();
        group.add(&list_box);
        root.append(&group);

        // Raw (unparseable) binds section
        let raw_group = adw::PreferencesGroup::builder()
            .title(t("kb-shortcuts-advanced"))
            .visible(false)
            .build();
        let raw_list_box = gtk::ListBox::builder()
            .selection_mode(gtk::SelectionMode::None)
            .css_classes(["boxed-list"])
            .build();
        raw_group.add(&raw_list_box);
        root.append(&raw_group);

        // Add button
        let add_button = gtk::Button::builder()
            .label(t("kb-shortcuts-add"))
            .css_classes(["suggested-action"])
            .halign(gtk::Align::Start)
            .build();
        root.append(&add_button);

        // Register search entries
        {
            let mut idx = search_index.borrow_mut();
            let page_title = t("settings-keyboard-shortcuts");
            idx.add_section(
                "keyboard-shortcuts",
                &page_title,
                &t("kb-shortcuts-custom"),
                "kb-shortcuts-custom",
                &group,
            );
        }

        let config_path = startup::niri_config_path();

        // Load initial entries
        let (initial_entries, initial_raw, parse_error) =
            match keyboard_shortcuts::load_binds(&config_path) {
                Ok(loaded) => (loaded.entries, loaded.raw, false),
                Err(e) => {
                    log::error!("[keyboard-shortcuts] Failed to load config: {e}");
                    error_state.set_description(Some(&e));
                    (Vec::new(), Vec::new(), true)
                }
            };

        let state = Rc::new(RefCell::new(ShortcutsPageState {
            entries: Vec::new(),
            raw_nodes: initial_raw.clone(),
            rows: Vec::new(),
            list_box,
            raw_list_box,
            empty_state,
            group,
            raw_group,
            error_state,
        }));

        if parse_error {
            state.borrow().error_state.set_visible(true);
            add_button.set_sensitive(false);
        } else {
            // Show raw binds (read-only)
            {
                let s = state.borrow();
                for raw_name in &initial_raw {
                    let row = adw::ActionRow::builder()
                        .title(raw_name)
                        .subtitle(t("kb-shortcuts-read-only"))
                        .build();
                    s.raw_list_box.append(&row);
                }
                s.raw_group.set_visible(!initial_raw.is_empty());
            }
            Self::reconcile_entries(&state, &initial_entries, &config_path, &root);
        }

        // Wire add button
        {
            let state = state.clone();
            let config_path = config_path.clone();
            let root_ref = root.clone();
            add_button.connect_clicked(move |btn| {
                let dialog = BindEditor::new(None);
                let state = state.clone();
                let config_path = config_path.clone();
                let root_ref = root_ref.clone();
                dialog.connect_confirmed(move |entry| {
                    let mut s = state.borrow_mut();
                    s.entries.push(entry);
                    let entries = s.entries.clone();
                    let raw = s.raw_nodes.clone();
                    drop(s);
                    Self::save_and_reconcile(&state, &entries, &raw, &config_path, &root_ref);
                });
                dialog.present(btn);
            });
        }

        Self { root }
    }

    fn reconcile_entries(
        state: &Rc<RefCell<ShortcutsPageState>>,
        entries: &[BindEntry],
        config_path: &std::path::Path,
        root: &gtk::Box,
    ) {
        let mut s = state.borrow_mut();

        // Clear existing rows
        for row in &s.rows {
            s.list_box.remove(&row.widget());
        }
        s.rows.clear();
        s.entries = entries.to_vec();

        // Build new rows
        for (idx, entry) in entries.iter().enumerate() {
            let props = BindRowProps {
                key_chord: entry.key_chord(),
                action_label: entry.action.label(),
                title: entry.hotkey_overlay_title.clone(),
                editable: true,
            };
            let row = BindRow::build(&props);

            let state_for_output = state.clone();
            let config_path = config_path.to_path_buf();
            let root_ref = root.clone();
            row.connect_output(move |output| match output {
                BindRowOutput::Edit => {
                    let s = state_for_output.borrow();
                    let current_entry = match s.entries.get(idx) {
                        Some(e) => e.clone(),
                        None => return,
                    };
                    let raw = s.raw_nodes.clone();
                    drop(s);

                    let dialog = BindEditor::new(Some(&current_entry));
                    let state = state_for_output.clone();
                    let config_path = config_path.clone();
                    let root_for_confirm = root_ref.clone();
                    dialog.connect_confirmed(move |updated_entry| {
                        let mut s = state.borrow_mut();
                        if idx < s.entries.len() {
                            s.entries[idx] = updated_entry;
                        }
                        let entries = s.entries.clone();
                        drop(s);
                        Self::save_and_reconcile(
                            &state,
                            &entries,
                            &raw,
                            &config_path,
                            &root_for_confirm,
                        );
                    });
                    dialog.present(&root_ref);
                }
                BindRowOutput::Delete => {
                    let mut s = state_for_output.borrow_mut();
                    if idx < s.entries.len() {
                        s.entries.remove(idx);
                    }
                    let entries = s.entries.clone();
                    let raw = s.raw_nodes.clone();
                    drop(s);
                    Self::save_and_reconcile(
                        &state_for_output,
                        &entries,
                        &raw,
                        &config_path,
                        &root_ref,
                    );
                }
            });

            s.list_box.append(&row.widget());
            s.rows.push(row);
        }

        // Toggle empty state
        let has_entries = !entries.is_empty();
        s.group.set_visible(has_entries);
        s.empty_state.set_visible(!has_entries && s.raw_nodes.is_empty());
        s.error_state.set_visible(false);
    }

    fn save_and_reconcile(
        state: &Rc<RefCell<ShortcutsPageState>>,
        entries: &[BindEntry],
        raw_nodes: &[String],
        config_path: &std::path::Path,
        root: &gtk::Box,
    ) {
        if let Err(e) = keyboard_shortcuts::save_binds(config_path, entries, raw_nodes) {
            log::error!("[keyboard-shortcuts] Failed to save config: {e}");
        }
        Self::reconcile_entries(state, entries, config_path, root);
    }
}
