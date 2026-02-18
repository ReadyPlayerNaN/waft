//! Layout row widget -- displays a single keyboard layout with drag handle, rename, and remove buttons.

#![allow(dead_code)]

use std::cell::RefCell;
use std::rc::Rc;

use adw::prelude::*;

/// Output events from layout row.
pub enum LayoutRowOutput {
    Remove(String),
    Rename(String),
}

type OutputCallback = Rc<RefCell<Option<Box<dyn Fn(LayoutRowOutput)>>>>;

/// Single layout row widget (ActionRow with drag handle prefix).
pub struct LayoutRow {
    pub root: adw::ActionRow,
    pub drag_handle_box: gtk::Box,
    output_cb: OutputCallback,
    rename_btn: gtk::Button,
}

impl LayoutRow {
    pub fn new(code: &str, full_name: &str) -> Self {
        let root = adw::ActionRow::builder()
            .title(full_name)
            .subtitle(code)
            .activatable(false)
            .build();

        // Drag handle box (prefix)
        let drag_handle_box = gtk::Box::builder()
            .orientation(gtk::Orientation::Horizontal)
            .valign(gtk::Align::Center)
            .css_classes(["drag-handle"])
            .build();

        let drag_icon = gtk::Image::builder()
            .icon_name("list-drag-handle-symbolic")
            .pixel_size(16)
            .build();

        drag_handle_box.append(&drag_icon);

        // Set cursor hint on the entire box
        drag_handle_box.set_cursor_from_name(Some("grab"));

        root.add_prefix(&drag_handle_box);

        // Rename button
        let rename_btn = gtk::Button::builder()
            .icon_name("document-edit-symbolic")
            .valign(gtk::Align::Center)
            .css_classes(["flat"])
            .tooltip_text("Rename layout")
            .visible(false)
            .build();
        root.add_suffix(&rename_btn);

        // Remove button
        let remove_btn = gtk::Button::builder()
            .icon_name("user-trash-symbolic")
            .valign(gtk::Align::Center)
            .css_classes(["flat"])
            .tooltip_text("Remove layout")
            .build();
        root.add_suffix(&remove_btn);

        let output_cb: OutputCallback = Rc::new(RefCell::new(None));

        // Connect remove button
        {
            let code_clone = code.to_string();
            let cb_clone = output_cb.clone();
            remove_btn.connect_clicked(move |_| {
                if let Some(ref callback) = *cb_clone.borrow() {
                    callback(LayoutRowOutput::Remove(code_clone.clone()));
                }
            });
        }

        // Connect rename button
        {
            let code_clone = code.to_string();
            let cb_clone = output_cb.clone();
            rename_btn.connect_clicked(move |_| {
                if let Some(ref callback) = *cb_clone.borrow() {
                    callback(LayoutRowOutput::Rename(code_clone.clone()));
                }
            });
        }

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
