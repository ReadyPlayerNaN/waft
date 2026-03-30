//! Individual notification card widget.
//!
//! Renders a single notification with icon, title, description, close button,
//! and optional action buttons. Supports left-click (default action), right-click
//! (dismiss), and animated show/hide via a Revealer.
//!
//! When `toast_ttl` is provided, a countdown progress bar is shown at the bottom
//! of the card. The bar ticks down and fires `TimedOut` when elapsed. Hovering
//! the card pauses the countdown.

use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::atomic::Ordering;

use gtk::prelude::*;

use waft_protocol::Urn;
use waft_protocol::entity::notification::{NotificationAction, NotificationIconHint};

use super::countdown_bar::{CountdownBarOutput, CountdownBarWidget};
use super::notification_markup;
use crate::icons::{Icon, IconWidget};

/// Type alias for output callback to reduce complexity.
type OutputCallback<T> = Rc<RefCell<Option<Box<dyn Fn(T)>>>>;

/// Output events from a notification card.
#[derive(Debug, Clone)]
pub enum NotificationCardOutput {
    ActionClick(Urn, String),
    Close(Urn),
    TimedOut(Urn),
}

/// A notification card for the panel.
pub struct NotificationCard {
    urn: Urn,
    pub root: gtk::Box,
    revealer: gtk::Revealer,
    title_label: gtk::Label,
    description_label: gtk::Label,
    expanded_label: gtk::Label,
    expand_revealer: gtk::Revealer,
    expand_button: gtk::Button,
    on_output: OutputCallback<NotificationCardOutput>,
    hidden: Rc<RefCell<bool>>,
    countdown_bar: Option<CountdownBarWidget>,
}

impl NotificationCard {
    pub fn new(
        urn: Urn,
        title: &str,
        description: &str,
        icon_hints: &[NotificationIconHint],
        actions: &[NotificationAction],
        toast_ttl: Option<u64>,
        window_resize_callback: Option<Rc<dyn Fn()>>,
    ) -> Self {
        let root = gtk::Box::new(gtk::Orientation::Vertical, 0);

        let revealer = gtk::Revealer::builder()
            .transition_type(gtk::RevealerTransitionType::SlideDown)
            .transition_duration(200)
            .reveal_child(false)
            .build();

        let on_output: OutputCallback<NotificationCardOutput> = Rc::new(RefCell::new(None));
        let hidden = Rc::new(RefCell::new(false));

        // Card box
        let card_box = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .css_classes(["notification-card", "card"])
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

        // Icon (32px)
        let icons = convert_icon_hints(icon_hints);
        let icon_widget = IconWidget::new(&icons, 32);

        // Content box
        let content_box = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .spacing(8)
            .css_classes(["notification-content"])
            .hexpand(true)
            .halign(gtk::Align::Fill)
            .build();

        // Title
        let prepared_title = notification_markup::prepare_title(title);
        let title_label = gtk::Label::builder()
            .css_classes(["heading"])
            .wrap(true)
            .wrap_mode(gtk::pango::WrapMode::WordChar)
            .xalign(0.0)
            .use_markup(true)
            .build();
        title_label.set_markup(&prepared_title);

        // Description (truncated to 3 lines)
        let prepared_desc = notification_markup::prepare_description(description);
        let description_label = gtk::Label::builder()
            .css_classes(["dim-label"])
            .wrap(true)
            .wrap_mode(gtk::pango::WrapMode::WordChar)
            .xalign(0.0)
            .use_markup(true)
            .lines(3)
            .ellipsize(gtk::pango::EllipsizeMode::End)
            .build();
        description_label.set_markup(&prepared_desc);

        // Expanded description (full text, hidden by default)
        let expanded_label = gtk::Label::builder()
            .css_classes(["dim-label"])
            .wrap(true)
            .wrap_mode(gtk::pango::WrapMode::WordChar)
            .xalign(0.0)
            .use_markup(true)
            .build();
        expanded_label.set_markup(&prepared_desc);

        let expand_revealer = gtk::Revealer::builder()
            .transition_type(gtk::RevealerTransitionType::SlideDown)
            .transition_duration(200)
            .reveal_child(false)
            .build();
        expand_revealer.set_child(Some(&expanded_label));

        let expand_button = gtk::Button::builder()
            .label("Show More")
            .css_classes(["flat", "notification-expand-btn"])
            .halign(gtk::Align::Start)
            .visible(false)
            .build();

        // Toggle expand/collapse on button click
        {
            let desc_label = description_label.clone();
            let exp_revealer = expand_revealer.clone();
            let btn = expand_button.clone();
            expand_button.connect_clicked(move |_| {
                let revealing = !exp_revealer.reveals_child();
                exp_revealer.set_reveal_child(revealing);
                desc_label.set_visible(!revealing);
                btn.set_label(if revealing { "Show Less" } else { "Show More" });
            });
        }

        content_box.append(&title_label);
        content_box.append(&description_label);
        content_box.append(&expand_revealer);
        content_box.append(&expand_button);

        // Check if description is ellipsized and show expand button
        {
            let btn = expand_button.clone();
            let label = description_label.clone();
            gtk::glib::idle_add_local_once(move || {
                if label.layout().is_ellipsized() {
                    btn.set_visible(true);
                }
            });
        }

        // Spacer
        let spacer = gtk::Box::builder().hexpand(true).build();

        // Close button
        let close_btn = gtk::Button::builder()
            .icon_name("window-close-symbolic")
            .css_classes(["flat", "circular", "notification-close"])
            .valign(gtk::Align::Start)
            .halign(gtk::Align::End)
            .build();

        header.append(icon_widget.widget());
        header.append(&content_box);
        header.append(&spacer);
        header.append(&close_btn);
        card_box.append(&header);

        // Action buttons
        let non_default_actions: Vec<_> = actions.iter().filter(|a| a.key != "default").collect();

        if !non_default_actions.is_empty() {
            let actions_box = gtk::Box::builder()
                .orientation(gtk::Orientation::Horizontal)
                .spacing(8)
                .margin_start(16)
                .margin_end(16)
                .margin_bottom(12)
                .halign(gtk::Align::End)
                .build();

            for action in &non_default_actions {
                let action_btn = gtk::Button::builder()
                    .label(&action.label)
                    .css_classes(["notification-action"])
                    .build();

                let action_key = action.key.clone();
                let urn_clone = urn.clone();
                let on_output_ref = on_output.clone();
                let hidden_ref = hidden.clone();
                action_btn.connect_clicked(move |_| {
                    if *hidden_ref.borrow() {
                        return;
                    }
                    if let Some(ref cb) = *on_output_ref.borrow() {
                        cb(NotificationCardOutput::ActionClick(
                            urn_clone.clone(),
                            action_key.clone(),
                        ));
                    }
                });

                actions_box.append(&action_btn);
            }

            card_box.append(&actions_box);
        }

        // Countdown bar (when toast_ttl is set)
        let countdown_bar = toast_ttl.map(|ttl_ms| {
            let bar = CountdownBarWidget::new(ttl_ms);
            card_box.append(&bar.root());

            // Wire Elapsed -> TimedOut output
            let on_output_timeout = on_output.clone();
            let urn_for_timeout = urn.clone();
            let hidden_for_timeout = hidden.clone();
            bar.connect_output(move |CountdownBarOutput::Elapsed| {
                if *hidden_for_timeout.borrow() {
                    return;
                }
                *hidden_for_timeout.borrow_mut() = true;
                if let Some(ref cb) = *on_output_timeout.borrow() {
                    cb(NotificationCardOutput::TimedOut(urn_for_timeout.clone()));
                }
            });

            bar
        });

        // Hover detection: pause/resume countdown on mouse enter/leave
        if let Some(ref bar) = countdown_bar {
            let running = bar.running_handle();
            let bar_root = bar.root();

            let running_enter = running.clone();
            let bar_root_enter = bar_root.clone();
            let running_leave = running;
            let bar_root_leave = bar_root;

            let motion = gtk::EventControllerMotion::new();
            motion.connect_enter(move |_, _, _| {
                running_enter.store(false, Ordering::SeqCst);
                bar_root_enter.add_css_class("paused");
            });
            motion.connect_leave(move |_| {
                running_leave.store(true, Ordering::SeqCst);
                bar_root_leave.remove_css_class("paused");
            });
            root.add_controller(motion);
        }

        revealer.set_child(Some(&card_box));
        root.append(&revealer);

        // Trigger window resize after reveal animation completes so the
        // layer-shell window recalculates its height. The Revealer itself
        // collapses to zero height when not revealing, so we don't need to
        // remove the card from the DOM here — that is handled by the group's
        // update() when the entity store confirms the removal.
        if let Some(callback) = window_resize_callback {
            let cb_for_card = callback.clone();
            revealer.connect_child_revealed_notify(move |_rev| {
                cb_for_card();
            });
            expand_revealer.connect_child_revealed_notify(move |_rev| {
                callback();
            });
        }

        // Close button handler
        {
            let on_output_ref = on_output.clone();
            let revealer_ref = revealer.clone();
            let urn_clone = urn.clone();
            let hidden_ref = hidden.clone();
            close_btn.connect_clicked(move |_| {
                if *hidden_ref.borrow() {
                    return;
                }
                *hidden_ref.borrow_mut() = true;
                revealer_ref.set_reveal_child(false);
                if let Some(ref cb) = *on_output_ref.borrow() {
                    cb(NotificationCardOutput::Close(urn_clone.clone()));
                }
            });
        }

        // Right-click to dismiss
        {
            let on_output_ref = on_output.clone();
            let revealer_ref = revealer.clone();
            let urn_clone = urn.clone();
            let hidden_ref = hidden.clone();
            let right_click = gtk::GestureClick::new();
            right_click.set_button(3);
            right_click.connect_pressed(move |_gesture, _n_press, _x, _y| {
                if *hidden_ref.borrow() {
                    return;
                }
                *hidden_ref.borrow_mut() = true;
                revealer_ref.set_reveal_child(false);
                if let Some(ref cb) = *on_output_ref.borrow() {
                    cb(NotificationCardOutput::Close(urn_clone.clone()));
                }
            });
            root.add_controller(right_click);
        }

        // Left-click for default action
        {
            let on_output_ref = on_output.clone();
            let revealer_ref = revealer.clone();
            let urn_clone = urn.clone();
            let hidden_ref = hidden.clone();
            let left_click = gtk::GestureClick::new();
            left_click.set_button(1);
            left_click.connect_pressed(move |gesture, _n_press, x, y| {
                if *hidden_ref.borrow() {
                    return;
                }

                // Don't fire default action when clicking interactive elements
                if let Some(widget) = gesture.widget()
                    && let Some(picked) = widget.pick(x, y, gtk::PickFlags::DEFAULT)
                {
                    let mut current: Option<gtk::Widget> = Some(picked);
                    while let Some(ref w) = current {
                        if w.downcast_ref::<gtk::Button>().is_some() {
                            return;
                        }
                        current = w.parent();
                    }
                }

                *hidden_ref.borrow_mut() = true;
                if let Some(ref cb) = *on_output_ref.borrow() {
                    cb(NotificationCardOutput::ActionClick(
                        urn_clone.clone(),
                        "default".to_string(),
                    ));
                    cb(NotificationCardOutput::Close(urn_clone.clone()));
                }
                revealer_ref.set_reveal_child(false);
            });
            root.add_controller(left_click);
        }

        Self {
            urn,
            root,
            revealer,
            title_label,
            description_label,
            expanded_label,
            expand_revealer,
            expand_button,
            on_output,
            hidden,
            countdown_bar,
        }
    }

    /// Set the callback for output events.
    pub fn connect_output<F>(&self, callback: F)
    where
        F: Fn(NotificationCardOutput) + 'static,
    {
        *self.on_output.borrow_mut() = Some(Box::new(callback));
    }

    /// Show the card with animation. Starts the countdown timer if present.
    pub fn show(&self) {
        self.root.set_visible(true);
        self.revealer.set_reveal_child(true);
        if let Some(ref bar) = self.countdown_bar {
            bar.start();
        }
    }

    /// Hide the card with animation and stop the countdown timer.
    pub fn hide_and_remove(&self) {
        if let Some(ref bar) = self.countdown_bar {
            bar.stop();
        }
        *self.hidden.borrow_mut() = true;
        self.revealer.set_reveal_child(false);
    }

    /// Update the card content.
    pub fn update(&self, title: &str, description: &str) {
        let prepared_title = notification_markup::prepare_title(title);
        let prepared_desc = notification_markup::prepare_description(description);
        self.title_label.set_markup(&prepared_title);
        self.description_label.set_markup(&prepared_desc);
        self.expanded_label.set_markup(&prepared_desc);

        // If expanded and new text no longer needs truncation, collapse
        let expand_revealer = self.expand_revealer.clone();
        let expand_button = self.expand_button.clone();
        let description_label = self.description_label.clone();
        gtk::glib::idle_add_local_once(move || {
            let is_ellipsized = description_label.layout().is_ellipsized();
            if !is_ellipsized && expand_revealer.reveals_child() {
                expand_revealer.set_reveal_child(false);
                description_label.set_visible(true);
                expand_button.set_label("Show More");
            }
            expand_button.set_visible(is_ellipsized);
        });
    }

    pub fn urn(&self) -> &Urn {
        &self.urn
    }

    /// Get a reference to the revealer for animation-complete callbacks.
    pub fn revealer(&self) -> &gtk::Revealer {
        &self.revealer
    }

    #[allow(dead_code)]
    pub fn widget(&self) -> &gtk::Box {
        &self.root
    }
}

/// Convert protocol notification icon hints to the generic Icon type used by IconWidget.
fn convert_icon_hints(hints: &[NotificationIconHint]) -> Vec<Icon> {
    hints
        .iter()
        .map(|h| match h {
            NotificationIconHint::Themed(name) => Icon::Themed(name.to_string()),
            NotificationIconHint::FilePath(path) => Icon::FilePath(PathBuf::from(path)),
            NotificationIconHint::Bytes(bytes) => Icon::Bytes(bytes.clone()),
        })
        .collect()
}
