//! XML-backed widget for a single keyboard shortcut row.

use std::cell::RefCell;
use std::rc::Rc;

use adw::prelude::*;

/// Input data for constructing or updating a bind row.
#[derive(Clone, PartialEq)]
pub struct BindRowProps {
    pub key_chord: String,
    pub action_label: String,
    /// Optional action type badge (e.g. "spawn"). None for niri actions.
    pub action_type: Option<String>,
    pub title: Option<String>,
    pub editable: bool,
}

/// Output events from a bind row.
#[derive(Debug, Clone)]
pub enum BindRowOutput {
    Edit,
    Delete,
}

type OutputCallback = Rc<RefCell<Option<Box<dyn Fn(BindRowOutput)>>>>;

/// A shortcut row with an XML-defined hierarchy.
pub struct BindRow {
    pub root: adw::ActionRow,
    action_type_label: gtk::Label,
    edit_button: gtk::Button,
    delete_button: gtk::Button,
    suffix_box: gtk::Box,
    output_cb: OutputCallback,
}

impl BindRow {
    pub fn build(props: &BindRowProps) -> Self {
        let builder = gtk::Builder::from_resource("/com/waft/settings/bind-row.ui");
        let root: adw::ActionRow = builder
            .object("root")
            .expect("bind-row.ui must contain root");
        let suffix_box: gtk::Box = builder
            .object("suffix_box")
            .expect("bind-row.ui must contain suffix_box");
        let action_type_label: gtk::Label = builder
            .object("action_type_label")
            .expect("bind-row.ui must contain action_type_label");
        let edit_button: gtk::Button = builder
            .object("edit_button")
            .expect("bind-row.ui must contain edit_button");
        let delete_button: gtk::Button = builder
            .object("delete_button")
            .expect("bind-row.ui must contain delete_button");
        let output_cb: OutputCallback = Rc::new(RefCell::new(None));

        {
            let output_cb = output_cb.clone();
            edit_button.connect_clicked(move |_| {
                if let Some(callback) = output_cb.borrow().as_ref() {
                    callback(BindRowOutput::Edit);
                }
            });
        }
        {
            let output_cb = output_cb.clone();
            delete_button.connect_clicked(move |_| {
                if let Some(callback) = output_cb.borrow().as_ref() {
                    callback(BindRowOutput::Delete);
                }
            });
        }

        let row = Self {
            root,
            action_type_label,
            edit_button,
            delete_button,
            suffix_box,
            output_cb,
        };
        row.update(props);
        row
    }

    pub fn update(&self, props: &BindRowProps) {
        let title = props.title.as_deref().unwrap_or(&props.action_label);
        self.root.set_title(title);
        self.root.set_subtitle(&props.key_chord);
        self.action_type_label
            .set_label(props.action_type.as_deref().unwrap_or_default());
        self.action_type_label
            .set_visible(props.action_type.is_some());
        self.suffix_box.set_visible(props.editable);
        self.edit_button.set_sensitive(props.editable);
        self.delete_button.set_sensitive(props.editable);
    }

    pub fn connect_output<F: Fn(BindRowOutput) + 'static>(&self, callback: F) {
        *self.output_cb.borrow_mut() = Some(Box::new(callback));
    }

    pub fn widget(&self) -> gtk::Widget {
        self.root.clone().upcast()
    }
}
