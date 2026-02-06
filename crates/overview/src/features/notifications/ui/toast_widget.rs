//! Pure GTK4 toast widget.
//!
//! Individual toast notification display with animation support.

use std::cell::RefCell;
use std::rc::Rc;

use gtk::prelude::*;

use super::notification_layout::{NotificationLayoutConfig, NotificationLayoutParts};
use crate::features::notifications::types::{NotificationAction, NotificationIcon};

/// Pure GTK4 toast widget - no Relm4 factory abstractions.
/// Manages its own revealer for animations and provides direct control over lifecycle.
pub struct ToastWidget {
    pub id: u64,
    pub root: gtk::Box,
    revealer: gtk::Revealer,
    layout: NotificationLayoutParts,
    hidden: Rc<RefCell<bool>>,
    remove_on_hide: Rc<RefCell<bool>>,
}

impl ToastWidget {
    pub fn new<F, A, H>(
        id: u64,
        title: &str,
        description: &str,
        icon_hints: Vec<NotificationIcon>,
        actions: Vec<NotificationAction>,
        toast_ttl: Option<u64>,
        on_close: F,
        on_action: A,
        on_hover_change: H,
    ) -> Self
    where
        F: Fn(u64) + 'static,
        A: Fn(u64, String) + 'static,
        H: Fn(bool) + Clone + 'static,
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

        // Build shared layout
        let on_action = Rc::new(on_action);
        let on_action_for_layout = on_action.clone();
        let layout = NotificationLayoutParts::build(
            NotificationLayoutConfig {
                id,
                title: title.to_string(),
                description: description.to_string(),
                icon_hints,
                actions,
                css_classes: vec!["toast", "card", "notification-card"],
                show_close_button: true,
                toast_ttl,
            },
            move |action_id, action_key| {
                on_action_for_layout(action_id, action_key);
            },
        );

        revealer.set_child(Some(&layout.card_box));
        root.append(&revealer);

        // Track hidden state for double-click guard
        let hidden = Rc::new(RefCell::new(true));

        // Track whether this widget should be removed when hidden
        // (vs staying in place for potential re-show)
        let remove_on_hide = Rc::new(RefCell::new(false));
        let remove_on_hide_clone = remove_on_hide.clone();

        // When revealer finishes collapsing, optionally remove widget from parent container
        // Only remove if explicitly marked for removal (TTL expired, dismissed)
        // Hidden widgets (slot limited) should stay in place for re-show
        let root_clone = root.clone();
        revealer.connect_child_revealed_notify(move |rev| {
            if !rev.is_child_revealed() && *remove_on_hide_clone.borrow() {
                // Defer widget removal to after current event processing completes
                // This prevents GTK CRITICAL errors when gesture handlers are still active
                let root_for_removal = root_clone.clone();
                gtk::glib::idle_add_local_once(move || {
                    if let Some(parent) = root_for_removal.parent()
                        && let Some(parent_box) = parent.downcast_ref::<gtk::Box>() {
                            parent_box.remove(&root_for_removal);
                        }
                });
            }
        });

        // Close button click handler
        let on_close = Rc::new(on_close);
        let hidden_clone = hidden.clone();
        let revealer_clone = revealer.clone();
        let on_close_clone = on_close.clone();
        layout.close_btn.connect_clicked(move |_| {
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
        let on_close_clone2 = on_close.clone();
        let right_click = gtk::GestureClick::new();
        right_click.set_button(3); // Right mouse button
        right_click.connect_pressed(move |_gesture, _n_press, _x, _y| {
            if *hidden_clone.borrow() {
                return;
            }
            *hidden_clone.borrow_mut() = true;
            revealer_clone.set_reveal_child(false); // Start hide animation immediately
            on_close_clone2(id);
        });
        root.add_controller(right_click);

        // Left-click anywhere on card to trigger default action and close
        let hidden_clone = hidden.clone();
        let revealer_clone = revealer.clone();
        let on_action_clone = on_action.clone();
        let left_click = gtk::GestureClick::new();
        left_click.set_button(1); // Left mouse button
        left_click.connect_pressed(move |gesture, _n_press, x, y| {
            if *hidden_clone.borrow() {
                return;
            }

            // Check if click is on an interactive element (button) - if so, let the button handle it
            if let Some(widget) = gesture.widget()
                && let Some(picked) = widget.pick(x, y, gtk::PickFlags::DEFAULT) {
                    // Walk up the widget tree to check if click was on a Button
                    let mut current: Option<gtk::Widget> = Some(picked);
                    while let Some(ref w) = current {
                        if w.downcast_ref::<gtk::Button>().is_some() {
                            return; // Click was on a button, don't trigger default action
                        }
                        current = w.parent();
                    }
                }

            *hidden_clone.borrow_mut() = true;
            on_action_clone(id, "default".to_string());
            revealer_clone.set_reveal_child(false); // Start hide animation
            on_close(id);
        });
        root.add_controller(left_click);

        // Hover detection for pausing countdown
        let motion = gtk::EventControllerMotion::new();
        let on_enter = on_hover_change.clone();
        let on_leave = on_hover_change;
        motion.connect_enter(move |_, _, _| on_enter(true));
        motion.connect_leave(move |_| on_leave(false));
        root.add_controller(motion);

        Self {
            id,
            root,
            revealer,
            layout,
            hidden,
            remove_on_hide,
        }
    }

    /// Show the toast with animation
    pub fn show(&self) {
        *self.hidden.borrow_mut() = false;
        *self.remove_on_hide.borrow_mut() = false; // Reset removal flag when showing
        self.root.set_visible(true); // Ensure root is visible before revealing
        self.revealer.set_reveal_child(true);
        self.start_countdown();
    }

    /// Start the countdown bar timer
    pub fn start_countdown(&self) {
        if let Some(ref bar) = self.layout.countdown_bar {
            bar.start();
        }
    }

    /// Pause the countdown bar timer
    pub fn pause_countdown(&self) {
        if let Some(ref bar) = self.layout.countdown_bar {
            bar.pause();
        }
    }

    /// Resume the countdown bar timer
    pub fn resume_countdown(&self) {
        if let Some(ref bar) = self.layout.countdown_bar {
            bar.resume();
        }
    }

    /// Hide the toast with animation (stays in container for potential re-show)
    pub fn hide(&self) {
        *self.hidden.borrow_mut() = true;
        self.revealer.set_reveal_child(false);
    }

    /// Hide and remove from container (for dismissed/expired toasts that won't return)
    pub fn hide_and_remove(&self) {
        *self.hidden.borrow_mut() = true;
        *self.remove_on_hide.borrow_mut() = true;
        self.revealer.set_reveal_child(false);
    }

    /// Check if currently hidden
    pub fn is_hidden(&self) -> bool {
        *self.hidden.borrow()
    }

    /// Update the toast content
    pub fn update(&self, title: &str, description: &str) {
        self.layout.update(title, description);
    }
}
