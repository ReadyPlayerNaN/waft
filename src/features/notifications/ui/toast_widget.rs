//! Pure GTK4 toast widget.
//!
//! Individual toast notification display with animation support.

use std::cell::RefCell;
use std::rc::Rc;

use gtk::prelude::*;

use super::icon::IconWidget;
use crate::features::notifications::types::NotificationIcon;

/// Pure GTK4 toast widget - no Relm4 factory abstractions.
/// Manages its own revealer for animations and provides direct control over lifecycle.
pub struct ToastWidget {
    pub id: u64,
    pub root: gtk::Box,
    revealer: gtk::Revealer,
    title_label: gtk::Label,
    description_label: gtk::Label,
    icon_widget: IconWidget,
    hidden: Rc<RefCell<bool>>,
}

impl ToastWidget {
    pub fn new<F>(
        id: u64,
        title: &str,
        description: &str,
        icon_hints: Vec<NotificationIcon>,
        on_close: F,
    ) -> Self
    where
        F: Fn(u64) + 'static,
    {
        // Root container
        let root = gtk::Box::new(gtk::Orientation::Vertical, 0);

        // Revealer for slide animation
        let revealer = gtk::Revealer::builder()
            .transition_type(gtk::RevealerTransitionType::SlideDown)
            .transition_duration(200)
            .reveal_child(false) // Start hidden, animate in
            .build();
        revealer.add_css_class("notification-card-revealer");

        // Card box
        let card_box = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .css_classes(["toast", "card", "notification-card"])
            .margin_top(8)
            .build();

        // Header
        let header = gtk::Box::builder()
            .orientation(gtk::Orientation::Horizontal)
            .hexpand(true)
            .spacing(12)
            .margin_start(16)
            .margin_end(16)
            .margin_top(16)
            .margin_bottom(16)
            .build();

        // Content box (title + description)
        let content_box = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .spacing(8)
            .css_classes(["notification-content"])
            .hexpand(true)
            .halign(gtk::Align::Fill)
            .build();

        // Title label
        let title_label = gtk::Label::builder()
            .css_classes(["heading"])
            .wrap(true)
            .wrap_mode(gtk::pango::WrapMode::WordChar)
            .xalign(0.0)
            .use_markup(true)
            .build();
        title_label.set_markup(title);

        // Description label
        let description_label = gtk::Label::builder()
            .css_classes(["dim-label"])
            .wrap(true)
            .wrap_mode(gtk::pango::WrapMode::WordChar)
            .xalign(0.0)
            .use_markup(true)
            .build();
        description_label.set_markup(description);

        content_box.append(&title_label);
        content_box.append(&description_label);

        // Icon widget
        let icon_widget = IconWidget::new(icon_hints);

        // Spacer
        let spacer = gtk::Box::builder().hexpand(true).build();

        // Close button
        let close_btn = gtk::Button::builder()
            .icon_name("window-close-symbolic")
            .css_classes(["flat", "circular", "notification-close"])
            .valign(gtk::Align::Start)
            .halign(gtk::Align::End)
            .build();

        // Append icon, then content
        header.append(icon_widget.widget());
        header.append(&content_box);
        header.append(&spacer);
        header.append(&close_btn);

        card_box.append(&header);
        revealer.set_child(Some(&card_box));
        root.append(&revealer);

        // Track hidden state for double-click guard
        let hidden = Rc::new(RefCell::new(true));

        // When revealer finishes collapsing, remove widget from parent container
        let root_clone = root.clone();
        revealer.connect_child_revealed_notify(move |rev| {
            if !rev.is_child_revealed() {
                // Remove from parent - this is the authoritative cleanup
                if let Some(parent) = root_clone.parent() {
                    if let Some(parent_box) = parent.downcast_ref::<gtk::Box>() {
                        parent_box.remove(&root_clone);
                    }
                }
            }
        });

        // Close button click handler
        let hidden_clone = hidden.clone();
        let revealer_clone = revealer.clone();
        let on_close = Rc::new(on_close);
        let on_close_clone = on_close.clone();
        close_btn.connect_clicked(move |_| {
            if *hidden_clone.borrow() {
                return; // Already hidden, ignore
            }
            *hidden_clone.borrow_mut() = true;
            revealer_clone.set_reveal_child(false); // Start hide animation immediately
            on_close_clone(id);
        });

        // Right-click anywhere on card to close
        let hidden_clone = hidden.clone();
        let revealer_clone = revealer.clone();
        let right_click = gtk::GestureClick::new();
        right_click.set_button(3); // Right mouse button
        right_click.connect_pressed(move |_gesture, _n_press, _x, _y| {
            if *hidden_clone.borrow() {
                return;
            }
            *hidden_clone.borrow_mut() = true;
            revealer_clone.set_reveal_child(false); // Start hide animation immediately
            on_close(id);
        });
        root.add_controller(right_click);

        Self {
            id,
            root,
            revealer,
            title_label,
            description_label,
            icon_widget,
            hidden,
        }
    }

    /// Show the toast with animation
    pub fn show(&self) {
        *self.hidden.borrow_mut() = false;
        self.root.set_visible(true); // Ensure root is visible before revealing
        self.revealer.set_reveal_child(true);
    }

    /// Hide the toast with animation
    pub fn hide(&self) {
        *self.hidden.borrow_mut() = true;
        self.revealer.set_reveal_child(false);
    }

    /// Check if currently hidden
    pub fn is_hidden(&self) -> bool {
        *self.hidden.borrow()
    }

    /// Update the toast content
    pub fn update(&self, title: &str, description: &str) {
        self.title_label.set_markup(title);
        self.description_label.set_markup(description);
    }
}
