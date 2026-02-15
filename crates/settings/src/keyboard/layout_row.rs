//! Layout row widget -- displays a single keyboard layout with remove and
//! move up/down buttons.

use std::cell::RefCell;
use std::rc::Rc;

use adw::prelude::*;

/// Output events from layout row.
pub enum LayoutRowOutput {
    Remove(String),
    MoveUp(String),
    MoveDown(String),
}

type OutputCallback = Rc<RefCell<Option<Box<dyn Fn(LayoutRowOutput)>>>>;

/// Single layout row widget.
pub struct LayoutRow {
    pub root: adw::ActionRow,
    output_cb: OutputCallback,
    move_up_btn: gtk::Button,
    move_down_btn: gtk::Button,
}

impl LayoutRow {
    pub fn new(code: &str, full_name: &str) -> Self {
        let row = adw::ActionRow::builder()
            .title(full_name)
            .subtitle(code)
            .activatable(false)
            .build();

        // Move up button
        let move_up_btn = gtk::Button::builder()
            .icon_name("go-up-symbolic")
            .valign(gtk::Align::Center)
            .css_classes(["flat"])
            .tooltip_text("Move up")
            .build();
        row.add_suffix(&move_up_btn);

        // Move down button
        let move_down_btn = gtk::Button::builder()
            .icon_name("go-down-symbolic")
            .valign(gtk::Align::Center)
            .css_classes(["flat"])
            .tooltip_text("Move down")
            .build();
        row.add_suffix(&move_down_btn);

        // Remove button
        let remove_btn = gtk::Button::builder()
            .icon_name("user-trash-symbolic")
            .valign(gtk::Align::Center)
            .css_classes(["flat"])
            .tooltip_text("Remove layout")
            .build();
        row.add_suffix(&remove_btn);

        let output_cb: OutputCallback = Rc::new(RefCell::new(None));

        {
            let code_clone = code.to_string();
            let cb_clone = output_cb.clone();
            remove_btn.connect_clicked(move |_| {
                if let Some(ref callback) = *cb_clone.borrow() {
                    callback(LayoutRowOutput::Remove(code_clone.clone()));
                }
            });
        }

        {
            let code_clone = code.to_string();
            let cb_clone = output_cb.clone();
            move_up_btn.connect_clicked(move |_| {
                if let Some(ref callback) = *cb_clone.borrow() {
                    callback(LayoutRowOutput::MoveUp(code_clone.clone()));
                }
            });
        }

        {
            let code_clone = code.to_string();
            let cb_clone = output_cb.clone();
            move_down_btn.connect_clicked(move |_| {
                if let Some(ref callback) = *cb_clone.borrow() {
                    callback(LayoutRowOutput::MoveDown(code_clone.clone()));
                }
            });
        }

        Self {
            root: row,
            output_cb,
            move_up_btn,
            move_down_btn,
        }
    }

    pub fn widget(&self) -> &gtk::Widget {
        self.root.upcast_ref()
    }

    /// Enable/disable move up button (disable for first row).
    pub fn set_can_move_up(&self, can: bool) {
        self.move_up_btn.set_sensitive(can);
    }

    /// Enable/disable move down button (disable for last row).
    pub fn set_can_move_down(&self, can: bool) {
        self.move_down_btn.set_sensitive(can);
    }

    pub fn connect_output<F: Fn(LayoutRowOutput) + 'static>(&self, callback: F) {
        *self.output_cb.borrow_mut() = Some(Box::new(callback));
    }
}
