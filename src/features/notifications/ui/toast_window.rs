//! Pure GTK4 Toast Window widget.
//!
//! A layer-shell window that displays toast notifications.

use std::cell::RefCell;
use std::rc::Rc;

use gtk::prelude::*;
use gtk4_layer_shell::LayerShell;

use super::toast_list::{ToastListOutput, ToastListWidget};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HPos {
    Left,
    Center,
    Right,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VPos {
    Top,
    Center,
    Bottom,
}

/// Output events from the toast window.
#[derive(Debug, Clone)]
pub enum ToastWindowOutput {
    ActionClick(u64, String),
    CardClick(u64),
    CardClose(u64),
    TimedOut(u64),
}

/// Pure GTK4 toast window.
pub struct ToastWindowWidget {
    pub window: gtk::Window,
    #[allow(dead_code)]
    toast_list: ToastListWidget,
    on_output: Rc<RefCell<Option<Box<dyn Fn(ToastWindowOutput)>>>>,
}

impl ToastWindowWidget {
    /// Create a new toast window at the given position.
    pub fn new(hpos: HPos, vpos: VPos) -> Self {
        let window = gtk::Window::builder()
            .title("")
            .decorated(false)
            .modal(false)
            .default_width(480)
            .hexpand(false)
            .resizable(false)
            .build();

        // Configure layer shell
        Self::configure_layer_shell(&window, hpos, vpos);

        let on_output: Rc<RefCell<Option<Box<dyn Fn(ToastWindowOutput)>>>> =
            Rc::new(RefCell::new(None));

        let toast_list = ToastListWidget::new();

        // Forward toast list events
        let on_output_ref = on_output.clone();
        toast_list.connect_output(move |event| {
            if let Some(ref callback) = *on_output_ref.borrow() {
                match event {
                    ToastListOutput::ActionClick(id, action) => {
                        callback(ToastWindowOutput::ActionClick(id, action));
                    }
                    ToastListOutput::CardClick(id) => {
                        callback(ToastWindowOutput::CardClick(id));
                    }
                    ToastListOutput::CardClose(id) => {
                        callback(ToastWindowOutput::CardClose(id));
                    }
                    ToastListOutput::CardTimedOut(id) => {
                        callback(ToastWindowOutput::TimedOut(id));
                    }
                }
            }
        });

        // Window content
        let content = gtk::Box::new(gtk::Orientation::Vertical, 6);
        content.append(toast_list.widget());

        window.set_child(Some(&content));

        // Start visible
        window.set_visible(true);

        Self {
            window,
            toast_list,
            on_output,
        }
    }

    /// Set the callback for output events.
    pub fn connect_output<F>(&self, callback: F)
    where
        F: Fn(ToastWindowOutput) + 'static,
    {
        *self.on_output.borrow_mut() = Some(Box::new(callback));
    }

    /// Get a reference to the window widget.
    pub fn widget(&self) -> &gtk::Window {
        &self.window
    }

    /// Show the window.
    pub fn present(&self) {
        self.window.present();
    }

    fn configure_layer_shell(window: &gtk::Window, hpos: HPos, vpos: VPos) {
        window.init_layer_shell();
        window.set_layer(gtk4_layer_shell::Layer::Overlay);
        window.set_keyboard_mode(gtk4_layer_shell::KeyboardMode::None);

        // Reset all anchors
        window.set_anchor(gtk4_layer_shell::Edge::Left, false);
        window.set_anchor(gtk4_layer_shell::Edge::Right, false);
        window.set_anchor(gtk4_layer_shell::Edge::Top, false);
        window.set_anchor(gtk4_layer_shell::Edge::Bottom, false);

        match hpos {
            HPos::Left => window.set_anchor(gtk4_layer_shell::Edge::Left, true),
            HPos::Right => window.set_anchor(gtk4_layer_shell::Edge::Right, true),
            HPos::Center => {}
        }

        match vpos {
            VPos::Top => window.set_anchor(gtk4_layer_shell::Edge::Top, true),
            VPos::Bottom => window.set_anchor(gtk4_layer_shell::Edge::Bottom, true),
            VPos::Center => {}
        }

        // Zero margins
        window.set_margin(gtk4_layer_shell::Edge::Left, 0);
        window.set_margin(gtk4_layer_shell::Edge::Right, 0);
        window.set_margin(gtk4_layer_shell::Edge::Top, 0);
        window.set_margin(gtk4_layer_shell::Edge::Bottom, 0);
    }
}
