use gtk::prelude::*;
use log::info;
use relm4::factory::FactoryHashMap;
use relm4::{ComponentParts, ComponentSender, SimpleComponent, gtk};
use std::sync::Arc;

use super::types::NotificationDisplay;
use super::ui::card_group::{
    NotificationCardGroup, NotificationCardGroupInit, NotificationCardGroupInput,
    NotificationCardGroupOutput,
};

pub struct NotificationsWidget {
    groups: FactoryHashMap<Arc<str>, NotificationCardGroup>,
}

pub struct NotificationsWidgetInit {
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
    Ingest(Arc<NotificationDisplay>),
    CardActionClick(u64, String),
    CardClick(u64),
    CardClose(u64),
    GroupCollapse(Arc<str>),
    GroupExpand(Arc<str>),
}

#[derive(Debug, Clone)]
pub enum NotificationsWidgetOutput {}

impl NotificationsWidget {
    fn create_group<'a>(
        groups: &'a mut FactoryHashMap<Arc<str>, NotificationCardGroup>,
        ntf: &NotificationDisplay,
    ) -> Option<&'a NotificationCardGroup> {
        info!(
            "Creating group for app_id: {}, app_label: {}",
            ntf.app_id(),
            ntf.app_label()
        );
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
        info!("Integrating notification: {:?}", notification);
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
        set_css_classes: &["clock-container"],

        gtk::Label {
            set_label: "Notifications",
            set_css_classes: &["title"],
        },

        #[local_ref]
        groups_widget -> gtk::Box {
            set_orientation: gtk::Orientation::Vertical,
            set_spacing: 2,
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
        info!(
            "Factory widget created, children count: {}",
            groups_widget.observe_children().n_items()
        );
        let model = Self { groups };
        let widgets = view_output!();
        ComponentParts { model, widgets }
    }

    fn update(&mut self, msg: Self::Input, _sender: ComponentSender<Self>) {
        match msg {
            Self::Input::Ingest(notification) => {
                info!(
                    "Received Ingest message for notification id: {}",
                    notification.id
                );
                self.ingest_notification(notification);
                info!(
                    "Factory widget children count after ingest: {}",
                    self.groups.widget().observe_children().n_items()
                );
            }
            Self::Input::CardActionClick(id, action) => {
                // TODO: Handle card action click
            }
            Self::Input::CardClick(id) => {
                // TODO: Handle card click
            }
            Self::Input::CardClose(id) => {
                self.groups
                    .broadcast(NotificationCardGroupInput::Remove(id));
            }
            Self::Input::GroupCollapse(group_id) => {
                // TODO: Handle group collapse
            }
            Self::Input::GroupExpand(group_id) => {
                // TODO: Handle group expand
            }
        }
    }
}
