//! Single pattern editor -- dumb widget.
//!
//! Presents dropdowns for field and operator, a text entry for value,
//! and a delete button. Parent reads state via `get_pattern()` on save.

use std::cell::RefCell;
use std::rc::Rc;

use adw::prelude::*;
use waft_protocol::entity::notification_filter::{MatchField, MatchOperator, Pattern};

use crate::i18n::t;

type OutputCallback = Rc<RefCell<Option<Box<dyn Fn(PatternRowOutput)>>>>;

fn field_labels() -> Vec<String> {
    vec![
        t("notif-field-app-name"),
        t("notif-field-app-id"),
        t("notif-field-title"),
        t("notif-field-body"),
        t("notif-field-category"),
        t("notif-field-urgency"),
        t("notif-field-workspace"),
    ]
}

const FIELD_VALUES: &[MatchField] = &[
    MatchField::AppName,
    MatchField::AppId,
    MatchField::Title,
    MatchField::Body,
    MatchField::Category,
    MatchField::Urgency,
    MatchField::Workspace,
];

fn operator_labels() -> Vec<String> {
    vec![
        t("notif-op-equals"),
        t("notif-op-not-equals"),
        t("notif-op-contains"),
        t("notif-op-not-contains"),
        t("notif-op-starts-with"),
        t("notif-op-not-starts-with"),
        t("notif-op-ends-with"),
        t("notif-op-not-ends-with"),
        t("notif-op-matches-regex"),
        t("notif-op-not-matches-regex"),
    ]
}

const OPERATOR_VALUES: &[MatchOperator] = &[
    MatchOperator::Equals,
    MatchOperator::NotEquals,
    MatchOperator::Contains,
    MatchOperator::NotContains,
    MatchOperator::StartsWith,
    MatchOperator::NotStartsWith,
    MatchOperator::EndsWith,
    MatchOperator::NotEndsWith,
    MatchOperator::MatchesRegex,
    MatchOperator::NotMatchesRegex,
];

pub enum PatternRowOutput {
    Delete,
}

pub struct PatternRow {
    pub root: gtk::Box,
    field_dropdown: gtk::DropDown,
    operator_dropdown: gtk::DropDown,
    value_entry: gtk::Entry,
    output_callback: OutputCallback,
}

impl PatternRow {
    pub fn new(pattern: &Pattern) -> Self {
        let output_callback: OutputCallback =
            Rc::new(RefCell::new(None));

        let root = gtk::Box::builder()
            .orientation(gtk::Orientation::Horizontal)
            .spacing(6)
            .build();

        let fl = field_labels();
        let fl_refs: Vec<&str> = fl.iter().map(std::string::String::as_str).collect();
        let field_model = gtk::StringList::new(&fl_refs);
        let field_dropdown = gtk::DropDown::builder()
            .model(&field_model)
            .selected(field_to_index(pattern.field))
            .valign(gtk::Align::Center)
            .build();
        root.append(&field_dropdown);

        let ol = operator_labels();
        let ol_refs: Vec<&str> = ol.iter().map(std::string::String::as_str).collect();
        let operator_model = gtk::StringList::new(&ol_refs);
        let operator_dropdown = gtk::DropDown::builder()
            .model(&operator_model)
            .selected(operator_to_index(pattern.operator))
            .valign(gtk::Align::Center)
            .build();
        root.append(&operator_dropdown);

        let value_entry = gtk::Entry::builder()
            .text(&pattern.value)
            .placeholder_text(t("notif-pattern-value"))
            .hexpand(true)
            .valign(gtk::Align::Center)
            .build();
        root.append(&value_entry);

        let delete_button = gtk::Button::builder()
            .icon_name("user-trash-symbolic")
            .css_classes(["flat", "destructive-action"])
            .valign(gtk::Align::Center)
            .build();

        {
            let cb = output_callback.clone();
            delete_button.connect_clicked(move |_| {
                if let Some(ref callback) = *cb.borrow() {
                    callback(PatternRowOutput::Delete);
                }
            });
        }

        root.append(&delete_button);

        Self {
            root,
            field_dropdown,
            operator_dropdown,
            value_entry,
            output_callback,
        }
    }

    pub fn get_pattern(&self) -> Pattern {
        let field_idx = self.field_dropdown.selected() as usize;
        let field = FIELD_VALUES
            .get(field_idx)
            .copied()
            .unwrap_or(MatchField::AppName);

        let op_idx = self.operator_dropdown.selected() as usize;
        let operator = OPERATOR_VALUES
            .get(op_idx)
            .copied()
            .unwrap_or(MatchOperator::Contains);

        let value = self.value_entry.text().to_string();

        Pattern {
            field,
            operator,
            value,
        }
    }

    pub fn connect_output<F: Fn(PatternRowOutput) + 'static>(&self, callback: F) {
        *self.output_callback.borrow_mut() = Some(Box::new(callback));
    }
}

fn field_to_index(field: MatchField) -> u32 {
    FIELD_VALUES
        .iter()
        .position(|f| *f == field)
        .unwrap_or(0) as u32
}

fn operator_to_index(op: MatchOperator) -> u32 {
    OPERATOR_VALUES
        .iter()
        .position(|o| *o == op)
        .unwrap_or(0) as u32
}
