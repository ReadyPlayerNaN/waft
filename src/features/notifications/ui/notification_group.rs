//! Notification group widget for the notifications panel.
//!
//! Groups notifications from the same application with expand/collapse functionality.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::Arc;

use gtk::prelude::*;
use uuid::Uuid;

use super::notification_card::{NotificationCard, NotificationCardOutput};
use crate::features::notifications::store::ItemLifecycle;
use crate::features::notifications::types::{NotificationAction, NotificationIcon};
use crate::menu_state::{MenuOp, MenuStore};
use crate::ui::icon::IconWidget;
use crate::ui::main_window::trigger_window_resize;
use crate::ui::menu_chevron::{MenuChevronProps, MenuChevronWidget};

/// Output events from a notification group.
#[derive(Debug, Clone)]
pub enum NotificationGroupOutput {
    ActionClick(u64, String),
    Close(u64),
}

/// Data for a single notification in the group.
#[derive(Debug, Clone)]
pub struct NotificationData {
    pub id: u64,
    pub title: Arc<str>,
    pub description: Arc<str>,
    pub icon_hints: Vec<NotificationIcon>,
    pub actions: Vec<NotificationAction>,
    pub lifecycle: ItemLifecycle,
}

/// A group of notifications from the same application.
pub struct NotificationGroup {
    pub app_ident: Arc<str>,
    pub root: gtk::Box,
    header: gtk::Box,
    app_title_label: gtk::Label,
    count_label: gtk::Label,
    expand_btn: gtk::Button,
    menu_chevron: MenuChevronWidget,
    top_card_container: gtk::Box,
    hidden_cards_revealer: gtk::Revealer,
    hidden_cards_container: gtk::Box,
    cards: Rc<RefCell<HashMap<u64, NotificationCard>>>,
    expanded: Rc<RefCell<bool>>,
    on_output: Rc<RefCell<Option<Box<dyn Fn(NotificationGroupOutput)>>>>,
    menu_id: String,
}

impl NotificationGroup {
    pub fn new(
        app_ident: Arc<str>,
        app_title: Arc<str>,
        icon_hints: Vec<NotificationIcon>,
        menu_store: Arc<MenuStore>,
    ) -> Self {
        // Generate unique ID for this menu
        let menu_id = Uuid::new_v4().to_string();
        let root = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .spacing(0)
            .css_classes(["notification-group"])
            .build();

        // Header
        let header = gtk::Box::builder()
            .orientation(gtk::Orientation::Horizontal)
            .spacing(8)
            .margin_start(8)
            .margin_end(8)
            .margin_top(8)
            .margin_bottom(4)
            .css_classes(["notification-group-header"])
            .build();

        // App icon (smaller than notification icon)
        let icon_widget = IconWidget::new(icon_hints);
        icon_widget.widget().set_pixel_size(16);
        icon_widget.widget().set_valign(gtk::Align::Center);

        // App title
        let app_title_label = gtk::Label::builder()
            .label(app_title.as_ref())
            .css_classes(["notification-group-title", "dim-label"])
            .hexpand(true)
            .xalign(0.0)
            .build();

        // Count badge
        let count_label = gtk::Label::builder()
            .css_classes(["notification-count-badge"])
            .visible(false)
            .build();

        let menu_chevron = MenuChevronWidget::new(MenuChevronProps { expanded: false });
        let expand_btn = gtk::Button::builder()
            .css_classes(["flat", "circular", "notification-expand"])
            .visible(false)
            .build();
        expand_btn.set_child(menu_chevron.widget());

        header.append(icon_widget.widget());
        header.append(&app_title_label);
        header.append(&count_label);
        header.append(&expand_btn);

        root.append(&header);

        // Top card container (always visible)
        let top_card_container = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .spacing(0)
            .build();
        root.append(&top_card_container);

        // Revealer for hidden cards
        let hidden_cards_revealer = gtk::Revealer::builder()
            .transition_type(gtk::RevealerTransitionType::SlideDown)
            .transition_duration(200)
            .reveal_child(false)
            .build();

        let hidden_cards_container = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .spacing(0)
            .build();

        hidden_cards_revealer.set_child(Some(&hidden_cards_container));
        root.append(&hidden_cards_revealer);

        let cards: Rc<RefCell<HashMap<u64, NotificationCard>>> =
            Rc::new(RefCell::new(HashMap::new()));
        let expanded = Rc::new(RefCell::new(false));
        let on_output: Rc<RefCell<Option<Box<dyn Fn(NotificationGroupOutput)>>>> =
            Rc::new(RefCell::new(None));

        // Menu chevron click handler
        let expanded_clone = expanded.clone();
        let menu_store_clone = menu_store.clone();
        let menu_id_clone = menu_id.clone();
        expand_btn.connect_clicked(move |_| {
            let is_currently_open = *expanded_clone.borrow();
            if is_currently_open {
                menu_store_clone.emit(MenuOp::CloseMenu(menu_id_clone.clone()));
            } else {
                menu_store_clone.emit(MenuOp::OpenMenu(menu_id_clone.clone()));
            }
        });

        // Trigger window resize when expand/collapse animation completes
        hidden_cards_revealer.connect_child_revealed_notify(move |_| {
            trigger_window_resize();
        });

        // Subscribe to menu store updates
        let hidden_cards_revealer_clone = hidden_cards_revealer.clone();
        let menu_chevron_clone = menu_chevron.clone();
        let expanded_clone = expanded.clone();
        let menu_store_clone = menu_store.clone();
        let menu_id_clone = menu_id.clone();
        menu_store.subscribe(move || {
            let state = menu_store_clone.get_state();
            let should_be_open = state.active_menu_id.as_ref() == Some(&menu_id_clone);

            *expanded_clone.borrow_mut() = should_be_open;
            menu_chevron_clone.set_expanded(should_be_open);
            hidden_cards_revealer_clone.set_reveal_child(should_be_open);
        });

        // Sync initial state
        {
            let state = menu_store.get_state();
            let should_be_open = state.active_menu_id.as_ref() == Some(&menu_id);
            *expanded.borrow_mut() = should_be_open;
            menu_chevron.set_expanded(should_be_open);
            hidden_cards_revealer.set_reveal_child(should_be_open);
        }

        Self {
            app_ident,
            root,
            header,
            app_title_label,
            count_label,
            expand_btn,
            menu_chevron,
            top_card_container,
            hidden_cards_revealer,
            hidden_cards_container,
            cards,
            expanded,
            on_output,
            menu_id,
        }
    }

    /// Set the callback for output events.
    pub fn connect_output<F>(&self, callback: F)
    where
        F: Fn(NotificationGroupOutput) + 'static,
    {
        *self.on_output.borrow_mut() = Some(Box::new(callback));
    }

    /// Update the group with new notification data.
    pub fn update(&self, notifications: &[NotificationData]) {
        log::debug!(
            "[notification_group] Updating with {} notifications",
            notifications.len()
        );

        let visible_notifications: Vec<_> = notifications
            .iter()
            .filter(|n| {
                let hidden = n.lifecycle.is_hidden();
                log::debug!(
                    "[notification_group] Notification {} lifecycle={:?} is_hidden={}",
                    n.id,
                    n.lifecycle,
                    hidden
                );
                !hidden
            })
            .collect();

        let total_count = visible_notifications.len();
        log::debug!(
            "[notification_group] {} visible after filtering",
            total_count
        );

        // Update count badge
        if total_count > 1 {
            self.count_label.set_label(&format!("{}", total_count));
            self.count_label.set_visible(true);
            self.expand_btn.set_visible(true);
        } else {
            self.count_label.set_visible(false);
            self.expand_btn.set_visible(false);
        }

        // Get current card IDs
        let current_ids: Vec<u64> = self.cards.borrow().keys().cloned().collect();
        let new_ids: Vec<u64> = visible_notifications.iter().map(|n| n.id).collect();

        // Remove cards that are no longer in the list
        for id in current_ids {
            if !new_ids.contains(&id) {
                if let Some(card) = self.cards.borrow_mut().remove(&id) {
                    card.hide();
                }
            }
        }

        // Create or update cards
        let on_output = self.on_output.clone();
        for (index, notif) in visible_notifications.iter().enumerate() {
            let mut cards = self.cards.borrow_mut();

            if !cards.contains_key(&notif.id) {
                // Create new card
                let card = NotificationCard::new(
                    notif.id,
                    &notif.title,
                    &notif.description,
                    notif.icon_hints.clone(),
                    notif.actions.clone(),
                );

                let on_output_clone = on_output.clone();
                card.connect_output(move |event| {
                    if let Some(ref callback) = *on_output_clone.borrow() {
                        match event {
                            NotificationCardOutput::ActionClick(id, action_key) => {
                                callback(NotificationGroupOutput::ActionClick(id, action_key));
                            }
                            NotificationCardOutput::Close(id) => {
                                callback(NotificationGroupOutput::Close(id));
                            }
                        }
                    }
                });

                // First card goes in top container, rest in hidden container
                if index == 0 {
                    self.top_card_container.append(&card.root);
                } else {
                    self.hidden_cards_container.append(&card.root);
                }

                card.show();
                cards.insert(notif.id, card);
            } else if let Some(card) = cards.get(&notif.id) {
                // Update existing card
                card.update(&notif.title, &notif.description);

                // Check if card needs to move between containers
                let should_be_top = index == 0;
                let is_in_top = card.root.parent().map_or(false, |p| {
                    p.eq(&self.top_card_container.clone().upcast::<gtk::Widget>())
                });

                if should_be_top && !is_in_top {
                    // Move to top container
                    if let Some(parent) = card.root.parent() {
                        if let Some(parent_box) = parent.downcast_ref::<gtk::Box>() {
                            parent_box.remove(&card.root);
                        }
                    }
                    self.top_card_container.prepend(&card.root);
                } else if !should_be_top && is_in_top {
                    // Move to hidden container
                    if let Some(parent) = card.root.parent() {
                        if let Some(parent_box) = parent.downcast_ref::<gtk::Box>() {
                            parent_box.remove(&card.root);
                        }
                    }
                    self.hidden_cards_container.append(&card.root);
                }
            }
        }

        // Update visibility
        self.root.set_visible(!visible_notifications.is_empty());
    }

    /// Get the number of visible notifications in this group.
    pub fn notification_count(&self) -> usize {
        self.cards.borrow().len()
    }

    /// Check if the group is expanded.
    pub fn is_expanded(&self) -> bool {
        *self.expanded.borrow()
    }

    /// Get the root widget.
    pub fn widget(&self) -> &gtk::Box {
        &self.root
    }

    /// Get a notification card by its ID.
    pub fn get_notification(&self, id: u64) -> Option<std::cell::Ref<'_, NotificationCard>> {
        let cards = self.cards.borrow();
        if cards.contains_key(&id) {
            Some(std::cell::Ref::map(cards, |c| c.get(&id).unwrap()))
        } else {
            None
        }
    }

    /// Get all notification IDs in this group for panel display.
    pub fn get_panel_notification_ids(&self) -> Vec<u64> {
        self.cards.borrow().keys().cloned().collect()
    }
}
