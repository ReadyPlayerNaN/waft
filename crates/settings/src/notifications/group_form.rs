//! Group create/edit form -- dumb widget.
//!
//! Presents a form with name, order, matcher editor, and save/cancel/delete buttons.
//! Emits `GroupFormOutput` events via `connect_output()`. On `SaveRequested`,
//! the parent calls `get_group()` to read the full form state including the
//! combinator tree.
//!
//! Uses `gtk::Box` as root so the combinator editor lives outside
//! `adw::PreferencesGroup`, keeping buttons interactive.

use std::cell::RefCell;
use std::rc::Rc;

use adw::prelude::*;
use waft_protocol::entity::notification_filter::{
    CombinatorOperator, MatchField, MatchOperator, NotificationGroup, Pattern, RuleCombinator,
    RuleNode,
};

use crate::i18n::t;
use crate::notifications::combinator_editor::CombinatorEditor;
use crate::notifications::id_from_name;

type OutputCallback = Rc<RefCell<Option<Box<dyn Fn(GroupFormOutput)>>>>;

pub enum GroupFormOutput {
    /// Parent should call `get_group()` to read full state.
    SaveRequested,
    Cancel,
    Delete(String),
}

pub struct GroupForm {
    pub root: gtk::Box,
    name_entry: adw::EntryRow,
    order_spin: adw::SpinRow,
    combinator_editor: CombinatorEditor,
    existing_id: Option<String>,
    output_callback: OutputCallback,
}

impl GroupForm {
    pub fn new(group: Option<&NotificationGroup>) -> Self {
        let output_callback: OutputCallback =
            Rc::new(RefCell::new(None));

        let edit_mode = group.is_some();
        let existing_id = group.map(|g| g.id.clone());

        let root = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .spacing(12)
            .build();

        // Name + order in a preferences group for consistent styling
        let fields_group = adw::PreferencesGroup::builder().build();

        let name_entry = adw::EntryRow::builder()
            .title(t("notif-group-name"))
            .text(group.map(|g| g.name.as_str()).unwrap_or(""))
            .show_apply_button(false)
            .build();
        fields_group.add(&name_entry);

        let adjustment = gtk::Adjustment::new(
            group.map(|g| g.order as f64).unwrap_or(0.0),
            0.0,
            999.0,
            1.0,
            10.0,
            0.0,
        );
        let order_spin = adw::SpinRow::builder()
            .title(t("notif-priority-order"))
            .adjustment(&adjustment)
            .build();
        fields_group.add(&order_spin);

        root.append(&fields_group);

        // Matcher editor -- directly in root, outside PreferencesGroup
        let default_matcher = RuleCombinator {
            operator: CombinatorOperator::And,
            children: vec![RuleNode::Pattern(Pattern {
                field: MatchField::AppName,
                operator: MatchOperator::Contains,
                value: String::new(),
            })],
        };
        let matcher = group.map(|g| &g.matcher).unwrap_or(&default_matcher);
        let combinator_editor = CombinatorEditor::new(matcher, 0);

        root.append(&combinator_editor.root);

        // Button row
        let button_box = gtk::Box::builder()
            .orientation(gtk::Orientation::Horizontal)
            .spacing(6)
            .build();

        let save_label = if edit_mode { t("notif-save") } else { t("notif-create") };
        let save_button = gtk::Button::builder()
            .label(&save_label)
            .css_classes(["pill", "suggested-action"])
            .build();
        button_box.append(&save_button);

        let cancel_button = gtk::Button::builder()
            .label(t("notif-cancel"))
            .css_classes(["pill"])
            .build();
        button_box.append(&cancel_button);

        let spacer = gtk::Box::builder().hexpand(true).build();
        button_box.append(&spacer);

        if edit_mode {
            let delete_button = gtk::Button::builder()
                .label(t("notif-delete"))
                .css_classes(["pill", "destructive-action"])
                .build();

            let id_for_delete = existing_id.clone().unwrap_or_default();
            let cb = output_callback.clone();
            delete_button.connect_clicked(move |_| {
                if let Some(ref callback) = *cb.borrow() {
                    callback(GroupFormOutput::Delete(id_for_delete.clone()));
                }
            });
            button_box.append(&delete_button);
        }

        root.append(&button_box);

        // Wire save — emits SaveRequested; parent reads via get_group()
        {
            let cb = output_callback.clone();
            save_button.connect_clicked(move |_| {
                if let Some(ref callback) = *cb.borrow() {
                    callback(GroupFormOutput::SaveRequested);
                }
            });
        }

        // Wire cancel
        {
            let cb = output_callback.clone();
            cancel_button.connect_clicked(move |_| {
                if let Some(ref callback) = *cb.borrow() {
                    callback(GroupFormOutput::Cancel);
                }
            });
        }

        Self {
            root,
            name_entry,
            order_spin,
            combinator_editor,
            existing_id,
            output_callback,
        }
    }

    /// Read the full group from form state, including the combinator tree.
    /// Returns `None` if the name is empty.
    pub fn get_group(&self) -> Option<NotificationGroup> {
        let name = self.name_entry.text().to_string();
        if name.trim().is_empty() {
            return None;
        }

        let id = self
            .existing_id
            .clone()
            .unwrap_or_else(|| id_from_name(&name));
        let order = self.order_spin.value() as u32;
        let matcher = self.combinator_editor.get_combinator();

        Some(NotificationGroup {
            id,
            name,
            order,
            matcher,
        })
    }

    pub fn connect_output<F: Fn(GroupFormOutput) + 'static>(&self, callback: F) {
        *self.output_callback.borrow_mut() = Some(Box::new(callback));
    }
}
