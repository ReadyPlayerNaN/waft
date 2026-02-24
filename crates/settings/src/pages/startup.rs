//! Startup settings page -- smart container.
//!
//! Reads niri `spawn-at-startup` entries from `~/.config/niri/config.kdl`
//! and allows adding, editing, and removing them.

use std::cell::RefCell;
use std::rc::Rc;

use adw::prelude::*;
use waft_ui_gtk::vdom::Component;

use crate::i18n::t;
use crate::search_index::SearchIndex;
use crate::startup::entry_dialog::EntryDialog;
use crate::startup::startup_row::{StartupRow, StartupRowOutput, StartupRowProps};
use crate::startup::{self, StartupEntry};

/// Smart container for the Startup settings page.
pub struct StartupPage {
    pub root: gtk::Box,
}

/// Internal mutable state for the Startup page.
struct StartupPageState {
    entries: Vec<StartupEntry>,
    rows: Vec<StartupRow>,
    list_box: gtk::ListBox,
    empty_state: adw::StatusPage,
    group: adw::PreferencesGroup,
    error_state: adw::StatusPage,
}

impl StartupPage {
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
            .title(t("startup-parse-error"))
            .visible(false)
            .build();
        root.append(&error_state);

        let empty_state = adw::StatusPage::builder()
            .icon_name("system-run-symbolic")
            .title(t("startup-no-entries"))
            .visible(false)
            .build();
        root.append(&empty_state);

        let group = adw::PreferencesGroup::builder()
            .title(t("startup-entries"))
            .visible(false)
            .build();

        let list_box = gtk::ListBox::builder()
            .selection_mode(gtk::SelectionMode::None)
            .css_classes(["boxed-list"])
            .build();
        group.add(&list_box);
        root.append(&group);

        // Add button
        let add_button = gtk::Button::builder()
            .label(t("startup-add"))
            .css_classes(["suggested-action"])
            .halign(gtk::Align::Start)
            .build();
        root.append(&add_button);

        // Register search entries
        {
            let mut idx = search_index.borrow_mut();
            let page_title = t("settings-startup");
            idx.add_section(
                "startup",
                &page_title,
                &t("startup-entries"),
                "startup-entries",
                &group,
            );
        }

        let config_path = startup::niri_config_path();

        // Load initial entries
        let (initial_entries, parse_error) = match startup::load_startup_entries(&config_path) {
            Ok(entries) => (entries, false),
            Err(e) => {
                log::error!("[startup] Failed to load config: {e}");
                error_state.set_description(Some(&e));
                (Vec::new(), true)
            }
        };

        let state = Rc::new(RefCell::new(StartupPageState {
            entries: Vec::new(),
            rows: Vec::new(),
            list_box,
            empty_state,
            group,
            error_state,
        }));

        if parse_error {
            state.borrow().error_state.set_visible(true);
            add_button.set_sensitive(false);
        } else {
            Self::reconcile_entries(&state, &initial_entries, &config_path, &root);
        }

        // Wire add button
        {
            let state = state.clone();
            let config_path = config_path.clone();
            let root_ref = root.clone();
            add_button.connect_clicked(move |btn| {
                let dialog = EntryDialog::new(None);
                let state = state.clone();
                let config_path = config_path.clone();
                let root_ref = root_ref.clone();
                dialog.connect_confirmed(move |entry| {
                    let mut s = state.borrow_mut();
                    s.entries.push(entry);
                    let entries = s.entries.clone();
                    drop(s);
                    Self::save_and_reconcile(&state, &entries, &config_path, &root_ref);
                });
                dialog.present(btn);
            });
        }

        Self { root }
    }

    fn reconcile_entries(
        state: &Rc<RefCell<StartupPageState>>,
        entries: &[StartupEntry],
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
            let props = StartupRowProps {
                command: entry.command.clone(),
                args: entry.args.clone(),
            };
            let row = StartupRow::build(&props);

            let state_for_output = state.clone();
            let config_path = config_path.to_path_buf();
            let root_ref = root.clone();
            row.connect_output(move |output| match output {
                StartupRowOutput::Edit => {
                    let s = state_for_output.borrow();
                    let current_entry = match s.entries.get(idx) {
                        Some(e) => e.clone(),
                        None => return,
                    };
                    drop(s);

                    let dialog = EntryDialog::new(Some(&current_entry));
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
                        Self::save_and_reconcile(&state, &entries, &config_path, &root_for_confirm);
                    });
                    dialog.present(&root_ref);
                }
                StartupRowOutput::Delete => {
                    let mut s = state_for_output.borrow_mut();
                    if idx < s.entries.len() {
                        s.entries.remove(idx);
                    }
                    let entries = s.entries.clone();
                    drop(s);
                    Self::save_and_reconcile(
                        &state_for_output,
                        &entries,
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
        s.empty_state.set_visible(!has_entries);
        s.error_state.set_visible(false);
    }

    fn save_and_reconcile(
        state: &Rc<RefCell<StartupPageState>>,
        entries: &[StartupEntry],
        config_path: &std::path::Path,
        root: &gtk::Box,
    ) {
        if let Err(e) = startup::save_startup_entries(config_path, entries) {
            log::error!("[startup] Failed to save config: {e}");
        }
        Self::reconcile_entries(state, entries, config_path, root);
    }
}
