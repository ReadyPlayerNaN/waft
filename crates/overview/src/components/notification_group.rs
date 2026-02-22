//! Notification group widget.
//!
//! Groups notifications from the same application with expand/collapse
//! functionality. The newest notification is always visible; additional
//! notifications are behind a revealer.

use std::cell::RefCell;
use std::collections::HashMap;
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::Arc;

use gtk::prelude::*;

use waft_protocol::Urn;
use waft_protocol::entity::notification::{NotificationAction, NotificationIconHint};
use waft_ui_gtk::icons::{Icon, IconWidget};
use waft_ui_gtk::widgets::menu_chevron::{MenuChevronProps, MenuChevronWidget};
use waft_ui_gtk::widgets::notification_card::{NotificationCard, NotificationCardOutput};

use crate::menu_state::{MenuOp, MenuStore};
use crate::ui::main_window::trigger_window_resize;

/// Type alias for output callback to reduce complexity.
type OutputCallback<T> = Rc<RefCell<Option<Box<dyn Fn(T)>>>>;

/// Output events from a notification group.
#[derive(Debug, Clone)]
pub enum NotificationGroupOutput {
    ActionClick(Urn, String),
    ClearAll(Vec<Urn>),
    Close(Urn),
}

/// Data for a single notification within a group.
pub struct NotificationData {
    pub urn: Urn,
    pub title: String,
    pub description: String,
    pub icon_hints: Vec<NotificationIconHint>,
    pub actions: Vec<NotificationAction>,
}

/// A group of notifications from the same application.
pub struct NotificationGroup {
    #[allow(dead_code)]
    app_id: String,
    root: gtk::Box,
    top_card_container: gtk::Box,
    #[allow(dead_code)]
    hidden_cards_revealer: gtk::Revealer,
    hidden_cards_container: gtk::Box,
    cards: Rc<RefCell<HashMap<String, NotificationCard>>>,
    clear_btn: gtk::Button,
    count_label: gtk::Label,
    expand_btn: gtk::Button,
    #[allow(dead_code)]
    menu_chevron: MenuChevronWidget,
    on_output: OutputCallback<NotificationGroupOutput>,
    #[allow(dead_code)]
    menu_id: String,
}

impl NotificationGroup {
    pub fn new(
        app_id: &str,
        app_title: &str,
        icon_hints: &[NotificationIconHint],
        menu_store: &Rc<MenuStore>,
    ) -> Self {
        let menu_id = uuid::Uuid::new_v4().to_string();

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

        // App icon (16px)
        let icons = convert_icon_hints(icon_hints);
        let icon_widget = IconWidget::new(icons, 16);

        // App title
        let app_title_label = gtk::Label::builder()
            .label(app_title)
            .css_classes(["notification-group-title", "dim-label"])
            .hexpand(true)
            .xalign(0.0)
            .build();

        // Clear all button
        let clear_btn = gtk::Button::builder()
            .icon_name("edit-clear-symbolic")
            .css_classes(["flat", "circular"])
            .visible(false)
            .build();

        // Count badge
        let count_label = gtk::Label::builder()
            .css_classes(["notification-count-badge"])
            .visible(false)
            .build();

        // Expand/collapse button with menu chevron
        let menu_chevron = MenuChevronWidget::new(MenuChevronProps { expanded: false });
        let expand_btn = gtk::Button::builder()
            .css_classes(["flat", "circular", "notification-expand"])
            .visible(false)
            .build();
        expand_btn.set_child(Some(&menu_chevron.root));

        header.append(icon_widget.widget());
        header.append(&app_title_label);
        header.append(&clear_btn);
        header.append(&count_label);
        header.append(&expand_btn);
        root.append(&header);

        // Top card container (always visible — newest notification)
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

        let cards: Rc<RefCell<HashMap<String, NotificationCard>>> =
            Rc::new(RefCell::new(HashMap::new()));
        let on_output: OutputCallback<NotificationGroupOutput> = Rc::new(RefCell::new(None));

        // Expand button → toggle via MenuStore
        {
            let menu_store_ref = menu_store.clone();
            let menu_id_clone = menu_id.clone();
            expand_btn.connect_clicked(move |_| {
                menu_store_ref.emit(MenuOp::OpenMenu(menu_id_clone.clone()));
            });
        }

        // Subscribe to MenuStore for expand/collapse state
        {
            let hidden_cards_revealer_ref = hidden_cards_revealer.clone();
            let menu_chevron_ref = menu_chevron.clone();
            let menu_id_clone = menu_id.clone();
            let menu_store_ref = menu_store.clone();
            menu_store.subscribe(move || {
                let state = menu_store_ref.get_state();
                let expanded = state.active_menu_id.as_ref() == Some(&menu_id_clone);
                hidden_cards_revealer_ref.set_reveal_child(expanded);
                menu_chevron_ref.set_expanded(expanded);
            });
        }

        // Trigger window resize when expand/collapse animation completes
        hidden_cards_revealer.connect_child_revealed_notify(move |_| {
            trigger_window_resize();
        });

        // Clear all button handler
        {
            let on_output_ref = on_output.clone();
            let cards_ref = cards.clone();
            clear_btn.connect_clicked(move |_| {
                let urns: Vec<Urn> = cards_ref
                    .borrow()
                    .values()
                    .map(|card| card.urn().clone())
                    .collect();
                if let Some(ref cb) = *on_output_ref.borrow() {
                    cb(NotificationGroupOutput::ClearAll(urns));
                }
            });
        }

        Self {
            app_id: app_id.to_string(),
            root,
            top_card_container,
            hidden_cards_revealer,
            hidden_cards_container,
            cards,
            clear_btn,
            count_label,
            expand_btn,
            menu_chevron,
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

    /// Update the group with current notification data.
    ///
    /// Creates new cards, updates existing ones, and removes absent ones.
    pub fn update(&self, notifications: &[NotificationData]) {
        let total = notifications.len();

        // Update header controls visibility
        if total > 1 {
            self.count_label.set_label(&format!("{}", total));
            self.count_label.set_visible(true);
            self.expand_btn.set_visible(true);
            self.clear_btn.set_visible(true);
        } else {
            self.count_label.set_visible(false);
            self.expand_btn.set_visible(false);
            self.clear_btn.set_visible(false);
        }

        // Collect current and new URN strings
        let current_keys: Vec<String> = self.cards.borrow().keys().cloned().collect();
        let new_keys: Vec<String> = notifications.iter().map(|n| n.urn.to_string()).collect();

        // Remove cards no longer present — remove from DOM directly
        for key in &current_keys {
            if !new_keys.contains(key)
                && let Some(card) = self.cards.borrow_mut().remove(key)
                && let Some(parent) = card.root.parent()
                && let Some(parent_box) = parent.downcast_ref::<gtk::Box>()
            {
                parent_box.remove(&card.root);
            }
        }

        // Create or update cards
        for (index, notif) in notifications.iter().enumerate() {
            let key = notif.urn.to_string();
            let mut cards = self.cards.borrow_mut();

            if let Some(existing) = cards.get(&key) {
                // Update existing card content
                existing.update(&notif.title, &notif.description);

                // Check if card needs to move between containers
                let should_be_top = index == 0;
                let is_in_top = existing.root.parent().is_some_and(|p| {
                    p.eq(&self.top_card_container.clone().upcast::<gtk::Widget>())
                });

                if should_be_top && !is_in_top {
                    if let Some(parent) = existing.root.parent()
                        && let Some(parent_box) = parent.downcast_ref::<gtk::Box>()
                    {
                        parent_box.remove(&existing.root);
                    }
                    self.top_card_container.prepend(&existing.root);
                } else if !should_be_top && is_in_top {
                    if let Some(parent) = existing.root.parent()
                        && let Some(parent_box) = parent.downcast_ref::<gtk::Box>()
                    {
                        parent_box.remove(&existing.root);
                    }
                    self.hidden_cards_container.append(&existing.root);
                }
            } else {
                // Create new card
                let card = NotificationCard::new(
                    notif.urn.clone(),
                    &notif.title,
                    &notif.description,
                    &notif.icon_hints,
                    &notif.actions,
                    None,
                    Some(Rc::new(trigger_window_resize)),
                );

                let on_output_ref = self.on_output.clone();
                card.connect_output(move |event| {
                    if let Some(ref cb) = *on_output_ref.borrow() {
                        match event {
                            NotificationCardOutput::ActionClick(urn, action_key) => {
                                cb(NotificationGroupOutput::ActionClick(urn, action_key));
                            }
                            NotificationCardOutput::Close(urn) => {
                                cb(NotificationGroupOutput::Close(urn));
                            }
                            NotificationCardOutput::TimedOut(urn) => {
                                cb(NotificationGroupOutput::Close(urn));
                            }
                        }
                    }
                });

                if index == 0 {
                    self.top_card_container.append(&card.root);
                } else {
                    self.hidden_cards_container.append(&card.root);
                }

                card.show();
                cards.insert(key, card);
            }
        }

        self.root.set_visible(!notifications.is_empty());
    }

    pub fn widget(&self) -> &gtk::Box {
        &self.root
    }

    #[allow(dead_code)]
    pub fn app_id(&self) -> &str {
        &self.app_id
    }
}

/// Convert protocol notification icon hints to the generic Icon type.
fn convert_icon_hints(hints: &[NotificationIconHint]) -> Vec<Icon> {
    hints
        .iter()
        .map(|h| match h {
            NotificationIconHint::Themed(name) => Icon::Themed(Arc::from(name.as_str())),
            NotificationIconHint::FilePath(path) => Icon::FilePath(Arc::new(PathBuf::from(path))),
            NotificationIconHint::Bytes(bytes) => Icon::Bytes(bytes.clone()),
        })
        .collect()
}
