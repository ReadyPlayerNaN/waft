use relm4::prelude::FactoryVecDeque;
use std::sync::Arc;

use gtk::prelude::*;
use relm4::{ComponentParts, ComponentSender, SimpleComponent, gtk};

use crate::ui::events::send_or_log;

use super::super::store::{REDUCER, State};

use super::card_group::{
    NotificationCardGroup, NotificationCardGroupInit, NotificationCardGroupInput,
    NotificationCardGroupOutput,
};

pub struct NotificationsWidget {
    expanded_group: Option<Arc<str>>,
    groups: FactoryVecDeque<NotificationCardGroup>,
    groups_map: Vec<Arc<str>>,
}

pub struct NotificationsWidgetInit {
    pub expanded_group: Option<Arc<str>>,
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
    }
}

#[derive(Debug, Clone)]
pub enum NotificationsWidgetInput {
    /// UI-driven events bubbling up from leaf `NotificationCard`s.
    CardActionClick(u64, String),
    CardClick(u64),
    GroupCollapse(Arc<str>),
    GroupExpand(Arc<str>),
    Remove(u64),
    StateChanged(State),
}

#[derive(Debug, Clone)]
pub enum NotificationsWidgetOutput {
    ActionClick(u64, String),
    CardClick(u64),
    CardClose(u64),
}

impl NotificationsWidget {
    fn get_index(target: &FactoryVecDeque<NotificationCardGroup>, id: &Arc<str>) -> Option<usize> {
        for (i, el) in target.iter().enumerate() {
            if &el.get_id() == id {
                return Some(i);
            }
        }
        None
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
        let groups: FactoryVecDeque<NotificationCardGroup> = FactoryVecDeque::builder()
            .launch(gtk::Box::default())
            .forward(sender.input_sender(), transform_notification_group_outputs);

        REDUCER.subscribe(sender.input_sender(), |s| {
            Self::Input::StateChanged(s.get_state().clone())
        });

        let groups_widget = groups.widget().clone();
        let model = Self {
            expanded_group: init.expanded_group,
            groups,
            groups_map: vec![],
        };
        let widgets = view_output!();
        ComponentParts { model, widgets }
    }

    fn update(&mut self, msg: Self::Input, sender: ComponentSender<Self>) {
        match msg {
            Self::Input::StateChanged(state) => {
                let groups = state.get_groups();
                for (group, lifecycle) in groups {
                    let existing = self.groups_map.iter().position(|id| id == group.get_id());
                    match existing {
                        Some(_) => {}
                        None => {
                            let gt = state.get_group_top(&group.get_id());
                            let gb = state.get_group_bottom(&group.get_id());
                            self.groups.guard().push_back(NotificationCardGroupInit {
                                expanded: false,
                                id: group.get_id().clone(),
                                title: group.get_title().clone(),
                                lifecycle: lifecycle.clone(),
                                top: gt
                                    .into_iter()
                                    .map(|(notification, lifecycle)| {
                                        (notification.clone(), lifecycle.clone())
                                    })
                                    .collect(),
                                rest: gb
                                    .into_iter()
                                    .map(|(notification, lifecycle)| {
                                        (notification.clone(), lifecycle.clone())
                                    })
                                    .collect(),
                            });
                            self.groups_map.push(group.get_id().clone());
                        }
                    }
                }
            }

            Self::Input::Remove(id) => {}

            Self::Input::CardActionClick(id, action) => {
                send_or_log(&sender, Self::Output::ActionClick(id, action));
            }
            Self::Input::CardClick(id) => {
                // Bubble up to the plugin; do not mutate UI state here (plugin is SoT).
                send_or_log(&sender, Self::Output::CardClick(id));
            }
            Self::Input::GroupCollapse(group_id) => {
                if self.expanded_group.as_ref() == Some(&group_id) {
                    if let Some(index) = Self::get_index(&self.groups, &group_id) {
                        self.groups
                            .send(index, NotificationCardGroupInput::Expand(false));
                    }
                    self.expanded_group = None;
                }
            }
            Self::Input::GroupExpand(group_id) => {
                if let Some(expanded_group) = &self.expanded_group {
                    if expanded_group != &group_id {
                        if let Some(index) = Self::get_index(&self.groups, expanded_group) {
                            self.groups
                                .send(index, NotificationCardGroupInput::Expand(false));
                        }
                    }
                }
                if let Some(index) = Self::get_index(&self.groups, &group_id) {
                    println!("INDEX: {}", index);
                    self.groups
                        .send(index, NotificationCardGroupInput::Expand(true));
                }
                self.expanded_group = Some(group_id);
            }
        }
    }
}
