use adw::prelude::*;
use log::{info, warn};
use relm4::factory::FactoryHashMap;
use relm4::gtk;
use relm4::prelude::*;
use std::collections::HashMap;

use super::super::types::NotificationDisplay;
use super::card::{NotificationCard, NotificationCardOutput};

pub struct NotificationCardGroup {
    expanded: bool,
    id: String,
    notifications: HashMap<u64, NotificationDisplay>,
    rest: FactoryHashMap<u64, NotificationCard>,
    title: String,
    top: FactoryHashMap<u64, NotificationCard>,
}

pub struct NotificationCardGroupInit {
    pub expanded: bool,
    pub id: String,
    pub notifications: Option<Vec<NotificationDisplay>>,
    pub title: String,
}

#[derive(Debug, Clone)]
pub enum NotificationCardGroupInput {
    ActionClick(u64, String),
    Expand(bool),
    ExpandClick,
    CardClick(u64),
    CardClose(u64),
    Ingest(NotificationDisplay),
    Remove(u64),
}

#[derive(Debug, Clone)]
pub enum NotificationCardGroupOutput {
    ActionClick(u64, String),
    CardClick(u64),
    CardClose(u64),
    Collapse(String),
    Expand(String),
}

fn transform_notification_card_outputs(msg: NotificationCardOutput) -> NotificationCardGroupInput {
    match msg {
        NotificationCardOutput::ActionClick(id, action) => {
            NotificationCardGroupInput::ActionClick(id, action)
        }
        NotificationCardOutput::CardClick(id) => NotificationCardGroupInput::CardClick(id),
        NotificationCardOutput::Close(id) => NotificationCardGroupInput::CardClose(id),
    }
}

#[relm4::factory(pub)]
impl FactoryComponent for NotificationCardGroup {
    type Index = String;
    type Init = NotificationCardGroupInit;
    type Input = NotificationCardGroupInput;
    type Output = NotificationCardGroupOutput;
    type CommandOutput = ();
    type ParentWidget = gtk::Box;

    view! {
      gtk::Box {
        set_orientation: gtk::Orientation::Vertical,

        gtk::Label {
          #[watch]
          set_label: &self.title,
        },

        #[local_ref]
        top -> gtk::Box {
          set_orientation: gtk::Orientation::Vertical,
          set_spacing: 6,
          set_margin_start: 12,
          set_margin_end: 12,
          set_margin_bottom: 8,
          set_margin_top: 8,
        },
        gtk::Box {
          set_orientation: gtk::Orientation::Vertical,
          set_spacing: 6,
          set_margin_start: 12,
          set_margin_end: 12,
          set_margin_bottom: 8,
          set_margin_top: 8,

          gtk::Button {
            set_label: "Expand",
            connect_clicked => Self::Input::ExpandClick,
          }
        },
        gtk::Revealer {
          #[watch]
          set_reveal_child: self.expanded,
          set_transition_type: gtk::RevealerTransitionType::SlideDown,

          #[local_ref]
          rest -> gtk::Box {
            set_orientation: gtk::Orientation::Vertical,
            set_spacing: 6,
            set_margin_start: 12,
            set_margin_end: 12,
            set_margin_bottom: 8,
            set_margin_top: 8,
          }
        }
      }
    }

    fn init_model(value: Self::Init, _index: &Self::Index, sender: FactorySender<Self>) -> Self {
        let ns = value.notifications.unwrap_or(vec![]);
        let map: HashMap<u64, NotificationDisplay> = ns.into_iter().map(|n| (n.id, n)).collect();
        let top = FactoryHashMap::builder()
            .launch(gtk::Box::default())
            .forward(sender.input_sender(), transform_notification_card_outputs);

        let rest = FactoryHashMap::builder()
            .launch(gtk::Box::default())
            .forward(sender.input_sender(), transform_notification_card_outputs);

        Self {
            id: value.id,
            expanded: value.expanded,
            title: value.title,
            notifications: map,
            top: top,
            rest: rest,
        }
    }

    fn init_widgets(
        &mut self,
        _index: &Self::Index,
        root: Self::Root,
        _returned_widget: &<Self::ParentWidget as relm4::factory::FactoryView>::ReturnedWidget,
        sender: FactorySender<Self>,
    ) -> Self::Widgets {
        let top = self.top.widget();
        let rest = self.rest.widget();
        let widgets = view_output!();
        widgets
    }

    fn update(&mut self, msg: Self::Input, sender: FactorySender<Self>) {
        match msg {
            Self::Input::Ingest(notification) => {
                info!("Ingesting notification: {:?}", notification);
                self.notifications
                    .insert(notification.id, notification.clone());
                self.top.insert(notification.id, notification);
                info!(
                    "NotificationCardGroup {} top factory now has {} items",
                    self.id,
                    self.top.len()
                );
                // Only move notifications to rest if we have more than 1 in top
                if self.top.len() > 1 {
                    if let Some((last_id, _last_notification)) = self.top.iter().last() {
                        let last_id = *last_id;
                        self.top.remove(&last_id);
                        match self.notifications.remove(&last_id) {
                            Some(n) => {
                                self.rest.insert(last_id, n);
                                info!("Moved notification {} to rest factory", last_id);
                            }
                            None => (),
                        };
                    }
                }
            }
            Self::Input::Remove(notification_id) => {
                self.notifications.remove(&notification_id);
                self.top.remove(&notification_id);
                self.rest.remove(&notification_id);
            }
            Self::Input::ActionClick(notification_id, action) => {
                sender.output(Self::Output::ActionClick(notification_id, action));
            }
            Self::Input::Expand(state) => {
                self.expanded = state;
            }
            Self::Input::ExpandClick => {
                if self.expanded {
                    sender.output(Self::Output::Collapse(self.id.clone()));
                } else {
                    sender.output(Self::Output::Expand(self.id.clone()));
                }
            }
            Self::Input::CardClick(notification_id) => {
                sender.output(Self::Output::CardClick(notification_id));
            }
            Self::Input::CardClose(notification_id) => {
                sender.output(Self::Output::CardClose(notification_id));
            }
        }
    }

    fn post_view(&mut self, widgets: &mut Self::Widgets, _sender: ComponentSender<Self>) {}
}
