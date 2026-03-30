//! Recursive combinator tree editor -- dumb widget.
//!
//! Displays a group of pattern rows combined with AND/OR logic.
//! Supports nested sub-groups and add/remove operations.
//! Parent reads the full tree via `get_combinator()` on save.

use std::cell::RefCell;
use std::rc::Rc;

use adw::prelude::*;
use waft_protocol::entity::notification_filter::{
    CombinatorOperator, MatchField, MatchOperator, Pattern, RuleCombinator, RuleNode,
};

use crate::i18n::t;
use crate::notifications::pattern_row::PatternRow;

type OutputCallback = Rc<RefCell<Option<Box<dyn Fn(CombinatorEditorOutput)>>>>;

pub enum CombinatorEditorOutput {
    Delete,
}

enum ChildEntry {
    Pattern(PatternRow),
    Combinator(CombinatorEditor),
}

pub struct CombinatorEditor {
    pub root: gtk::Box,
    operator_dropdown: gtk::DropDown,
    children: Rc<RefCell<Vec<ChildEntry>>>,
    output_callback: OutputCallback,
}

impl CombinatorEditor {
    pub fn new(combinator: &RuleCombinator, depth: usize) -> Self {
        let output_callback: OutputCallback =
            Rc::new(RefCell::new(None));

        let root = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .spacing(6)
            .build();

        // Header row: operator dropdown + action buttons
        let header = gtk::Box::builder()
            .orientation(gtk::Orientation::Horizontal)
            .spacing(6)
            .build();

        let op_labels = [t("notif-match-all-and"), t("notif-match-any-or")];
        let op_refs: Vec<&str> = op_labels.iter().map(std::string::String::as_str).collect();
        let operator_model = gtk::StringList::new(&op_refs);
        let operator_dropdown = gtk::DropDown::builder()
            .model(&operator_model)
            .selected(match combinator.operator {
                CombinatorOperator::And => 0,
                CombinatorOperator::Or => 1,
            })
            .valign(gtk::Align::Center)
            .build();
        header.append(&operator_dropdown);

        let add_rule_button = gtk::Button::builder()
            .label(t("notif-add-rule"))
            .css_classes(["flat", "suggested-action"])
            .valign(gtk::Align::Center)
            .build();
        header.append(&add_rule_button);

        let add_subgroup_button = gtk::Button::builder()
            .label(t("notif-add-subgroup"))
            .css_classes(["flat"])
            .valign(gtk::Align::Center)
            .build();
        header.append(&add_subgroup_button);

        if depth > 0 {
            let delete_button = gtk::Button::builder()
                .icon_name("user-trash-symbolic")
                .css_classes(["flat", "destructive-action"])
                .valign(gtk::Align::Center)
                .build();
            {
                let cb = output_callback.clone();
                delete_button.connect_clicked(move |_| {
                    if let Some(ref callback) = *cb.borrow() {
                        callback(CombinatorEditorOutput::Delete);
                    }
                });
            }
            header.append(&delete_button);
        }

        root.append(&header);

        // Children container with indentation
        let children_box = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .spacing(6)
            .margin_start(24)
            .build();
        root.append(&children_box);

        let children: Rc<RefCell<Vec<ChildEntry>>> = Rc::new(RefCell::new(Vec::new()));

        // Build initial children from combinator data
        for child in &combinator.children {
            match child {
                RuleNode::Pattern(pattern) => {
                    let row = PatternRow::new(pattern);
                    children_box.append(&row.root);
                    Self::wire_pattern_delete(&row, &children, &children_box);
                    children.borrow_mut().push(ChildEntry::Pattern(row));
                }
                RuleNode::Combinator(sub) => {
                    let editor = CombinatorEditor::new(sub, depth + 1);
                    children_box.append(&editor.root);
                    Self::wire_combinator_delete(&editor, &children, &children_box);
                    children.borrow_mut().push(ChildEntry::Combinator(editor));
                }
            }
        }

        // Wire "Add Rule" button
        {
            let children_ref = children.clone();
            let box_ref = children_box.clone();
            add_rule_button.connect_clicked(move |_| {
                let default_pattern = Pattern {
                    field: MatchField::AppName,
                    operator: MatchOperator::Contains,
                    value: String::new(),
                };
                let row = PatternRow::new(&default_pattern);
                box_ref.append(&row.root);
                Self::wire_pattern_delete(&row, &children_ref, &box_ref);
                children_ref.borrow_mut().push(ChildEntry::Pattern(row));
            });
        }

        // Wire "Add Sub-group" button
        {
            let children_ref = children.clone();
            let box_ref = children_box.clone();
            let child_depth = depth + 1;
            add_subgroup_button.connect_clicked(move |_| {
                let empty_combinator = RuleCombinator {
                    operator: CombinatorOperator::And,
                    children: Vec::new(),
                };
                let editor = CombinatorEditor::new(&empty_combinator, child_depth);
                box_ref.append(&editor.root);
                Self::wire_combinator_delete(&editor, &children_ref, &box_ref);
                children_ref
                    .borrow_mut()
                    .push(ChildEntry::Combinator(editor));
            });
        }

        Self {
            root,
            operator_dropdown,
            children,
            output_callback,
        }
    }

    pub fn get_combinator(&self) -> RuleCombinator {
        let operator = match self.operator_dropdown.selected() {
            1 => CombinatorOperator::Or,
            _ => CombinatorOperator::And,
        };

        let children = self
            .children
            .borrow()
            .iter()
            .map(|child| match child {
                ChildEntry::Pattern(row) => RuleNode::Pattern(row.get_pattern()),
                ChildEntry::Combinator(editor) => {
                    RuleNode::Combinator(editor.get_combinator())
                }
            })
            .collect();

        RuleCombinator { operator, children }
    }

    pub fn connect_output<F: Fn(CombinatorEditorOutput) + 'static>(&self, callback: F) {
        *self.output_callback.borrow_mut() = Some(Box::new(callback));
    }

    fn wire_pattern_delete(
        row: &PatternRow,
        children: &Rc<RefCell<Vec<ChildEntry>>>,
        container: &gtk::Box,
    ) {
        let root_widget = row.root.clone();
        let children_ref = children.clone();
        let container_ref = container.clone();
        row.connect_output(move |output| match output {
            crate::notifications::pattern_row::PatternRowOutput::Delete => {
                container_ref.remove(&root_widget);
                let mut entries = children_ref.borrow_mut();
                entries.retain(|entry| {
                    if let ChildEntry::Pattern(r) = entry {
                        return r.root != root_widget;
                    }
                    true
                });
            }
        });
    }

    fn wire_combinator_delete(
        editor: &CombinatorEditor,
        children: &Rc<RefCell<Vec<ChildEntry>>>,
        container: &gtk::Box,
    ) {
        let root_widget = editor.root.clone();
        let children_ref = children.clone();
        let container_ref = container.clone();
        editor.connect_output(move |output| match output {
            CombinatorEditorOutput::Delete => {
                container_ref.remove(&root_widget);
                let mut entries = children_ref.borrow_mut();
                entries.retain(|entry| {
                    if let ChildEntry::Combinator(e) = entry {
                        return e.root != root_widget;
                    }
                    true
                });
            }
        });
    }
}
