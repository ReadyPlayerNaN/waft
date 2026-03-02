//! Keyboard Shortcuts settings page -- smart container.
//!
//! Reads niri `binds { }` entries from `~/.config/niri/config.kdl`
//! and allows adding, editing, and removing keyboard shortcuts.
//! Entries are grouped by action category.

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::rc::Rc;

use adw::prelude::*;
use waft_ui_gtk::vdom::Component;

use crate::i18n::t;
use crate::keyboard_shortcuts::bind_editor::BindEditor;
use crate::keyboard_shortcuts::bind_row::{BindRow, BindRowOutput, BindRowProps};
use crate::keyboard_shortcuts::{self, BindAction, BindEntry, action_category};
use crate::search_index::SearchIndex;
use crate::kdl_config;

/// Ordered list of categories for display. Categories not in this list
/// appear at the end sorted alphabetically.
const CATEGORY_ORDER: &[&str] = &[
    "Custom",
    "Focus",
    "Window",
    "Workspace",
    "Layout",
    "Session",
    "Display",
    "Screenshot",
    "Other",
];

/// Return the i18n key for a category name.
fn category_i18n_key(category: &str) -> String {
    match category {
        "Custom" => "kb-shortcuts-cat-custom".to_string(),
        "Focus" => "kb-shortcuts-cat-focus".to_string(),
        "Window" => "kb-shortcuts-cat-window".to_string(),
        "Workspace" => "kb-shortcuts-cat-workspace".to_string(),
        "Layout" => "kb-shortcuts-cat-layout".to_string(),
        "Session" => "kb-shortcuts-cat-session".to_string(),
        "Display" => "kb-shortcuts-cat-display".to_string(),
        "Screenshot" => "kb-shortcuts-cat-screenshot".to_string(),
        "Other" => "kb-shortcuts-cat-other".to_string(),
        _ => "kb-shortcuts-cat-other".to_string(),
    }
}

/// Sort key for category ordering.
fn category_sort_key(category: &str) -> usize {
    CATEGORY_ORDER
        .iter()
        .position(|c| *c == category)
        .unwrap_or(CATEGORY_ORDER.len())
}

/// Smart container for the Keyboard Shortcuts settings page.
pub struct KeyboardShortcutsPage {
    pub root: gtk::Box,
}

/// A category group with its PreferencesGroup and ListBox.
struct CategoryGroup {
    group: adw::PreferencesGroup,
    /// Kept alive; rows are appended during reconciliation.
    #[allow(dead_code)]
    list_box: gtk::ListBox,
}

/// Internal mutable state.
struct ShortcutsPageState {
    entries: Vec<BindEntry>,
    raw_nodes: Vec<String>,
    rows: Vec<BindRow>,
    /// Category groups in display order: (category_name, group widgets).
    category_groups: Vec<(String, CategoryGroup)>,
    raw_list_box: gtk::ListBox,
    empty_state: adw::StatusPage,
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
                &root,
            );
        }

        let config_path = kdl_config::niri_config_path();

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
            category_groups: Vec::new(),
            raw_list_box,
            empty_state,
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

        // Clear existing rows from their list boxes
        for row in &s.rows {
            if let Some(parent) = row.widget().parent() {
                if let Some(list_box) = parent.downcast_ref::<gtk::ListBox>() {
                    list_box.remove(&row.widget());
                }
            }
        }
        s.rows.clear();

        // Remove old category groups from root
        for (_, cat_group) in &s.category_groups {
            root.remove(&cat_group.group);
        }
        s.category_groups.clear();

        s.entries = entries.to_vec();

        // Group entries by category, preserving flat index
        let mut categories: BTreeMap<String, Vec<(usize, &BindEntry)>> = BTreeMap::new();
        for (flat_idx, entry) in entries.iter().enumerate() {
            let cat = action_category(&entry.action).to_string();
            categories.entry(cat).or_default().push((flat_idx, entry));
        }

        // Sort categories by display order
        let mut sorted_cats: Vec<(String, Vec<(usize, &BindEntry)>)> =
            categories.into_iter().collect();
        sorted_cats.sort_by_key(|(cat, _)| category_sort_key(cat));

        // Create groups and rows
        // Insert groups before the raw_group (which stays at the end before add button)
        for (cat_name, cat_entries) in &sorted_cats {
            let i18n_key = category_i18n_key(cat_name);
            let group = adw::PreferencesGroup::builder()
                .title(t(&i18n_key))
                .build();
            let list_box = gtk::ListBox::builder()
                .selection_mode(gtk::SelectionMode::None)
                .css_classes(["boxed-list"])
                .build();
            group.add(&list_box);

            // Insert before raw_group: after the last category group, or after empty_state
            if let Some((_, last_cat)) = s.category_groups.last() {
                root.insert_child_after(&group, Some(&last_cat.group));
            } else {
                root.insert_child_after(&group, Some(&s.empty_state));
            }

            for &(flat_idx, entry) in cat_entries {
                let (action_label, action_type) = match &entry.action {
                    BindAction::Spawn { command, .. } => {
                        // Show only the command basename (no "spawn" prefix)
                        let basename = std::path::Path::new(command)
                            .file_name()
                            .map(|n| n.to_string_lossy().to_string())
                            .unwrap_or_else(|| command.clone());
                        (basename, Some(t("kb-shortcuts-type-spawn")))
                    }
                    BindAction::NiriAction { name, .. } => (name.clone(), None),
                };
                let props = BindRowProps {
                    key_chord: entry.key_chord(),
                    action_label,
                    action_type,
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
                        let current_entry = match s.entries.get(flat_idx) {
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
                            if flat_idx < s.entries.len() {
                                s.entries[flat_idx] = updated_entry;
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
                        if flat_idx < s.entries.len() {
                            s.entries.remove(flat_idx);
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

                list_box.append(&row.widget());
                s.rows.push(row);
            }

            s.category_groups.push((cat_name.clone(), CategoryGroup { group, list_box }));
        }

        // Toggle empty state
        let has_entries = !entries.is_empty();
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
