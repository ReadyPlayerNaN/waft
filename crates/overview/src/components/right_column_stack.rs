//! Right column stack component.
//!
//! Wraps right-column content in an `adw::ViewStack` with "controls" and "exit"
//! pages. Wires pre-created toggle buttons to switch between pages.

use adw::prelude::*;

use crate::ui::main_window::trigger_window_resize;

/// A tabbable right column with "controls" and "exit" pages.
pub struct RightColumnStackComponent {
    stack: adw::ViewStack,
}

impl RightColumnStackComponent {
    /// Create the stack and wire the provided toggle buttons to switch pages.
    ///
    /// The buttons must already be linked into a toggle group before calling this.
    pub fn new(
        controls_child: gtk::Widget,
        exit_child: gtk::Widget,
        controls_btn: &gtk::ToggleButton,
        exit_btn: &gtk::ToggleButton,
    ) -> Self {
        let stack = adw::ViewStack::builder().build();
        stack.add_named(&controls_child, Some("controls"));
        stack.add_named(&exit_child, Some("exit"));

        // Wire toggle buttons to switch stack pages
        {
            let stack_ref = stack.clone();
            controls_btn.connect_toggled(move |btn| {
                if btn.is_active() {
                    stack_ref.set_visible_child_name("controls");
                    trigger_window_resize();
                }
            });
        }
        {
            let stack_ref = stack.clone();
            exit_btn.connect_toggled(move |btn| {
                if btn.is_active() {
                    stack_ref.set_visible_child_name("exit");
                    trigger_window_resize();
                }
            });
        }

        Self { stack }
    }

    pub fn widget(&self) -> gtk::Widget {
        self.stack.clone().upcast()
    }
}
