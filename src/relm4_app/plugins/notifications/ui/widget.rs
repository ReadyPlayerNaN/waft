use gtk::prelude::*;
use log::info;
use relm4::factory::FactoryHashMap;
use relm4::{ComponentParts, ComponentSender, SimpleComponent, gtk};
use std::sync::Arc;

use super::super::types::NotificationDisplay;
use super::card_group::{
    NotificationCardGroup, NotificationCardGroupInit, NotificationCardGroupInput,
    NotificationCardGroupOutput,
};

pub struct NotificationsWidget {
    expanded_group: Option<Arc<str>>,
    groups: FactoryHashMap<Arc<str>, NotificationCardGroup>,
}

pub struct NotificationsWidgetInit {
    pub expanded_group: Option<Arc<str>>,
    pub notifications: Option<Vec<Arc<NotificationDisplay>>>,
}

fn transform_notification_group_outputs(
    msg: NotificationCardGroupOutput,
) -> NotificationsWidgetInput {
    match msg {
        NotificationCardGroupOutput::ActionClick(id, action) => {
            NotificationsWidgetInput::CardActionClick(id, action)
        }
        NotificationCardGroupOutput::Collapse(group_id) => {
            NotificationsWidgetInput::GroupCollapse(group_id)
        }
        NotificationCardGroupOutput::Expand(group_id) => {
            NotificationsWidgetInput::GroupExpand(group_id)
        }
        NotificationCardGroupOutput::CardClick(id) => NotificationsWidgetInput::CardClick(id),
        NotificationCardGroupOutput::CardClose(id) => NotificationsWidgetInput::CardClose(id),
    }
}

#[derive(Debug, Clone)]
pub enum NotificationsWidgetInput {
    /// Plugin-driven: ingest or update a notification (the plugin is the source of truth).
    Ingest(Arc<NotificationDisplay>),

    /// Plugin-driven: remove a notification from the rendered UI.
    Remove(u64),

    /// UI-driven events bubbling up from leaf `NotificationCard`s.
    CardActionClick(u64, String),
    CardClick(u64),
    CardClose(u64),

    GroupCollapse(Arc<str>),
    GroupExpand(Arc<str>),
}

#[derive(Debug, Clone)]
pub enum NotificationsWidgetOutput {
    ActionClick(u64, String),
    CardClick(u64),
    CardClose(u64),
}

impl NotificationsWidget {
    fn create_group<'a>(
        groups: &'a mut FactoryHashMap<Arc<str>, NotificationCardGroup>,
        ntf: &NotificationDisplay,
    ) -> Option<&'a NotificationCardGroup> {
        groups.insert(
            ntf.app_id(),
            NotificationCardGroupInit {
                expanded: false,
                id: ntf.app_id().clone(),
                notifications: None,
                title: ntf.app_label().clone(),
            },
        );
        let result = groups.get(&ntf.app_id());
        info!("Group created successfully: {}", result.is_some());
        result
    }

    fn ensure_group_existence(
        groups: &mut FactoryHashMap<Arc<str>, NotificationCardGroup>,
        ntf: &Arc<NotificationDisplay>,
    ) {
        if !groups.get(&ntf.app_id()).is_some() {
            Self::create_group(groups, ntf);
        }
    }

    fn integrate_notification(
        groups: &mut FactoryHashMap<Arc<str>, NotificationCardGroup>,
        notification: Arc<NotificationDisplay>,
    ) {
        Self::ensure_group_existence(groups, &notification);
        groups.send(
            &notification.app_id(),
            NotificationCardGroupInput::Ingest(notification),
        )
    }

    fn integrate_notifications(
        groups: &mut FactoryHashMap<Arc<str>, NotificationCardGroup>,
        notifications: Vec<Arc<NotificationDisplay>>,
    ) {
        for notification in notifications {
            Self::integrate_notification(groups, notification);
        }
    }

    fn ingest_notification(&mut self, notification: Arc<NotificationDisplay>) {
        Self::integrate_notification(&mut self.groups, notification);
    }
}

#[relm4::component(pub)]
impl SimpleComponent for NotificationsWidget {
    type Init = NotificationsWidgetInit;
    type Input = NotificationsWidgetInput;
    type Output = NotificationsWidgetOutput;

    view! {
      gtk::Box {
        set_orientation: gtk::Orientation::Vertical,
        set_spacing: 2,
        set_css_classes: &["notifications-widget"],

        gtk::Label {
            set_label: "Notifications",
            set_css_classes: &["heading", "title"],
            set_xalign: 0.0,
        },

        #[local_ref]
        groups_widget -> gtk::Box {
            set_orientation: gtk::Orientation::Vertical,
            set_spacing: 4,
            set_css_classes: &["notification-group"],
        }
      }
    }

    fn init(
        init: Self::Init,
        root: Self::Root,
        sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        let mut groups = FactoryHashMap::builder()
            .launch(gtk::Box::default())
            .forward(sender.input_sender(), transform_notification_group_outputs);

        match init.notifications {
            Some(notifications) => {
                info!("Initializing with {} notifications", notifications.len());
                Self::integrate_notifications(&mut groups, notifications);
            }
            None => {
                info!("Initializing with no notifications");
            }
        }

        let groups_widget = groups.widget().clone();
        let model = Self {
            expanded_group: init.expanded_group,
            groups,
        };
        let widgets = view_output!();
        ComponentParts { model, widgets }
    }

    fn update(&mut self, msg: Self::Input, sender: ComponentSender<Self>) {
        match msg {
            Self::Input::Ingest(notification) => {
                self.ingest_notification(notification);
            }

            Self::Input::Remove(id) => {
                // Plugin-driven reconciliation: remove from whichever group currently renders it.
                self.groups
                    .broadcast(NotificationCardGroupInput::Remove(id));
            }

            Self::Input::CardActionClick(id, action) => {
                // Bubble up to the plugin; do not mutate UI state here (plugin is SoT).
                let _ = sender.output(Self::Output::ActionClick(id, action));
            }
            Self::Input::CardClick(id) => {
                // Bubble up to the plugin; do not mutate UI state here (plugin is SoT).
                let _ = sender.output(Self::Output::CardClick(id));
            }
            Self::Input::CardClose(id) => {
                // Bubble up to the plugin; the plugin decides whether/when to remove and will
                // send `NotificationsWidgetInput::Remove(id)` back down.
                let _ = sender.output(Self::Output::CardClose(id));
            }

            Self::Input::GroupCollapse(group_id) => {
                if self.expanded_group.as_ref() == Some(&group_id) {
                    self.groups
                        .send(&group_id, NotificationCardGroupInput::Expand(false));
                    self.expanded_group = None;
                }
            }
            Self::Input::GroupExpand(group_id) => {
                if let Some(expanded_group) = &self.expanded_group {
                    if expanded_group != &group_id {
                        self.groups
                            .send(expanded_group, NotificationCardGroupInput::Expand(false));
                    }
                }
                self.groups
                    .send(&group_id, NotificationCardGroupInput::Expand(true));
                self.expanded_group = Some(group_id);
            }
        }
    }
}
