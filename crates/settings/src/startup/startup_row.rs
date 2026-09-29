//! XML-backed row for a niri startup entry.

use std::cell::RefCell;
use std::rc::Rc;

use adw::prelude::*;

use crate::i18n::t;

/// Input data for constructing or updating a startup row.
#[derive(Clone, PartialEq)]
pub struct StartupRowProps {
    pub command: String,
    pub args: Vec<String>,
}

/// Output events from a startup row.
#[derive(Debug, Clone)]
pub enum StartupRowOutput {
    Edit,
    Delete,
}

type OutputCallback = Rc<RefCell<Option<Box<dyn Fn(StartupRowOutput)>>>>;

/// Startup entry row with XML-defined structure and Rust-owned callbacks.
pub struct StartupRow {
    pub root: adw::ActionRow,
    output_cb: OutputCallback,
}

impl StartupRow {
    pub fn build(props: &StartupRowProps) -> Self {
        let builder = gtk::Builder::from_resource("/com/waft/settings/startup-row.ui");
        let root: adw::ActionRow = builder
            .object("root")
            .expect("startup-row.ui must contain root");
        let edit_button: gtk::Button = builder
            .object("edit_button")
            .expect("startup-row.ui must contain edit_button");
        let delete_button: gtk::Button = builder
            .object("delete_button")
            .expect("startup-row.ui must contain delete_button");
        edit_button.set_label(&t("startup-edit"));
        delete_button.set_label(&t("startup-delete"));

        let output_cb: OutputCallback = Rc::new(RefCell::new(None));
        {
            let output_cb = output_cb.clone();
            edit_button.connect_clicked(move |_| {
                if let Some(callback) = output_cb.borrow().as_ref() {
                    callback(StartupRowOutput::Edit);
                }
            });
        }
        {
            let output_cb = output_cb.clone();
            delete_button.connect_clicked(move |_| {
                if let Some(callback) = output_cb.borrow().as_ref() {
                    callback(StartupRowOutput::Delete);
                }
            });
        }

        let row = Self { root, output_cb };
        row.update(props);
        row
    }

    pub fn update(&self, props: &StartupRowProps) {
        self.root.set_title(&props.command);
        let subtitle = props.args.join(" ");
        self.root.set_subtitle(&subtitle);
    }

    pub fn connect_output<F: Fn(StartupRowOutput) + 'static>(&self, callback: F) {
        *self.output_cb.borrow_mut() = Some(Box::new(callback));
    }

    pub fn widget(&self) -> gtk::Widget {
        self.root.clone().upcast()
    }
}
