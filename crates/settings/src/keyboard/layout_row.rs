//! XML-backed row for a configured keyboard layout.

#![allow(dead_code)]

use std::cell::RefCell;
use std::rc::Rc;

use adw::prelude::*;
use waft_ui_gtk::icons::IconWidget;

/// Output events from layout row.
pub enum LayoutRowOutput {
    Remove(String),
    Rename(String),
}

type OutputCallback = Rc<RefCell<Option<Box<dyn Fn(LayoutRowOutput)>>>>;

/// Single layout row with a drag handle and action buttons.
pub struct LayoutRow {
    pub root: adw::ActionRow,
    pub drag_handle_box: gtk::Box,
    output_cb: OutputCallback,
    rename_btn: gtk::Button,
}

impl LayoutRow {
    pub fn new(code: &str, full_name: &str) -> Self {
        let builder = gtk::Builder::from_resource("/com/waft/settings/layout-row.ui");
        let root: adw::ActionRow = builder
            .object("root")
            .expect("layout-row.ui must contain root");
        let drag_handle_box: gtk::Box = builder
            .object("drag_handle_box")
            .expect("layout-row.ui must contain drag_handle_box");
        let rename_btn: gtk::Button = builder
            .object("rename_button")
            .expect("layout-row.ui must contain rename_button");
        let remove_btn: gtk::Button = builder
            .object("remove_button")
            .expect("layout-row.ui must contain remove_button");

        root.set_title(full_name);
        root.set_subtitle(code);
        drag_handle_box.set_cursor_from_name(Some("grab"));
        let drag_icon = IconWidget::from_name("list-drag-handle-symbolic", 16);
        drag_handle_box.append(drag_icon.widget());

        let output_cb: OutputCallback = Rc::new(RefCell::new(None));
        let code_for_remove = code.to_string();
        let cb_for_remove = output_cb.clone();
        remove_btn.connect_clicked(move |_| {
            if let Some(ref callback) = *cb_for_remove.borrow() {
                callback(LayoutRowOutput::Remove(code_for_remove.clone()));
            }
        });

        let code_for_rename = code.to_string();
        let cb_for_rename = output_cb.clone();
        rename_btn.connect_clicked(move |_| {
            if let Some(ref callback) = *cb_for_rename.borrow() {
                callback(LayoutRowOutput::Rename(code_for_rename.clone()));
            }
        });

        Self {
            root,
            drag_handle_box,
            output_cb,
            rename_btn,
        }
    }

    /// Show/hide the rename button based on whether renaming is supported.
    pub fn set_can_rename(&self, can: bool) {
        self.rename_btn.set_visible(can);
    }

    pub fn connect_output<F: Fn(LayoutRowOutput) + 'static>(&self, callback: F) {
        *self.output_cb.borrow_mut() = Some(Box::new(callback));
    }
}
