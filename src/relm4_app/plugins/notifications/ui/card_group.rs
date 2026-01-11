use adw::prelude::*;
use log::info;
use relm4::factory::FactoryVecDeque;
use relm4::gtk;
use relm4::prelude::*;
use std::collections::HashMap;
use std::sync::Arc;

use super::super::types::NotificationDisplay;
use super::card::{NotificationCard, NotificationCardInit, NotificationCardOutput};

pub struct NotificationCardGroup {
    expanded: bool,
    id: Arc<str>,
    notifications: HashMap<u64, Arc<NotificationDisplay>>,
    rest: FactoryVecDeque<NotificationCard>,
    title: Arc<str>,
    top: FactoryVecDeque<NotificationCard>,
}

pub struct NotificationCardGroupInit {
    pub expanded: bool,
    pub id: Arc<str>,
    pub notifications: Option<Vec<Arc<NotificationDisplay>>>,
    pub title: Arc<str>,
}

#[derive(Debug, Clone)]
pub enum NotificationCardGroupInput {
    ActionClick(u64, String),
    Expand(bool),
    ExpandClick,
    CardClick(u64),
    CardClose(u64),
    Ingest(Arc<NotificationDisplay>),
    Remove(u64),
    TimedOut(u64),
}

#[derive(Debug, Clone)]
pub enum NotificationCardGroupOutput {
    ActionClick(u64, String),
    CardClick(u64),
    CardClose(u64),
    Collapse(Arc<str>),
    Expand(Arc<str>),
}

fn transform_notification_card_outputs(msg: NotificationCardOutput) -> NotificationCardGroupInput {
    match msg {
        NotificationCardOutput::ActionClick(id, action) => {
            NotificationCardGroupInput::ActionClick(id, action)
        }
        NotificationCardOutput::CardClick(id) => NotificationCardGroupInput::CardClick(id),
        NotificationCardOutput::Close(id) => NotificationCardGroupInput::CardClose(id),
        NotificationCardOutput::TimedOut(id) => NotificationCardGroupInput::TimedOut(id),
    }
}

#[relm4::factory(pub)]
impl FactoryComponent for NotificationCardGroup {
    type Index = Arc<str>;
    type Init = NotificationCardGroupInit;
    type Input = NotificationCardGroupInput;
    type Output = NotificationCardGroupOutput;
    type CommandOutput = ();
    type ParentWidget = gtk::Box;

    view! {
      gtk::Revealer {
        #[watch]
        set_reveal_child: self.top.len() + self.rest.len() > 0,
        gtk::Box {
          set_orientation: gtk::Orientation::Vertical,
          set_spacing: 8,

          gtk::Label {
            #[watch]
            set_label: &format!("{} ({})", self.title, self.top.len() + self.rest.len()),
            set_xalign: 0.0,
          },

          #[local_ref]
          top -> gtk::Box {
            set_orientation: gtk::Orientation::Vertical,
            set_spacing: 6,
          },

          gtk::Revealer {
            #[watch]
            set_reveal_child: self.rest.len() > 0,
            set_transition_type: gtk::RevealerTransitionType::SlideDown,

            gtk::Box {
              set_orientation: gtk::Orientation::Vertical,
              set_spacing: 6,

              gtk::Button {
                #[watch]
                set_label: &format!("{} {} more", match self.expanded {
                  true => "Hide",
                  false => "Show",
                }, self.rest.len()),
                set_halign: gtk::Align::Start,
                set_hexpand: false,
                connect_clicked => Self::Input::ExpandClick,
              },

              gtk::Revealer {
                #[watch]
                set_reveal_child: self.expanded,
                set_transition_type: gtk::RevealerTransitionType::SlideDown,

                #[local_ref]
                rest -> gtk::Box {
                  set_orientation: gtk::Orientation::Vertical,
                  set_spacing: 6,
                }
              }
            }
          }
        }
      }
    }

    fn init_model(value: Self::Init, _index: &Self::Index, sender: FactorySender<Self>) -> Self {
        let ns = value.notifications.unwrap_or(vec![]);
        let map: HashMap<u64, Arc<NotificationDisplay>> =
            ns.into_iter().map(|n| (n.id, n)).collect();
        let top = FactoryVecDeque::builder()
            .launch(gtk::Box::default())
            .forward(sender.input_sender(), transform_notification_card_outputs);

        let rest = FactoryVecDeque::builder()
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
                let id = notification.id;
                self.notifications.insert(id, notification.clone());

                // Add to top (newest first)
                self.top.guard().push_front(NotificationCardInit {
                    countdown: false,
                    notification: notification.clone(),
                });

                info!(
                    "NotificationCardGroup {} top factory now has {} items",
                    self.id,
                    self.top.len()
                );

                // Only move notifications to rest if we have more than 1 in top
                if self.top.len() > 1 {
                    // Remove the last item (oldest in top)
                    if let Some(card) = self.top.guard().pop_back() {
                        let last_id = card.notification.id;
                        // Move to rest (newest in rest)
                        if let Some(n) = self.notifications.get(&last_id) {
                            self.rest.guard().push_front(NotificationCardInit {
                                countdown: false,
                                notification: n.clone(),
                            });
                            info!("Moved notification {} to rest factory", last_id);
                        }
                    }
                }
            }
            Self::Input::Remove(notification_id) => {
                self.notifications.remove(&notification_id);

                // Find and remove from top
                let top_idx = self
                    .top
                    .guard()
                    .iter()
                    .position(|c| c.notification.id == notification_id);
                if let Some(idx) = top_idx {
                    self.top.guard().remove(idx);
                }

                // Find and remove from rest
                let rest_idx = self
                    .rest
                    .guard()
                    .iter()
                    .position(|c| c.notification.id == notification_id);
                if let Some(idx) = rest_idx {
                    self.rest.guard().remove(idx);
                }

                // Promote from rest to top if top is empty
                if self.top.len() == 0 {
                    // Take the first item from rest (newest in rest)
                    if let Some(card) = self.rest.guard().pop_front() {
                        let id = card.notification.id;
                        if let Some(n) = self.notifications.get(&id) {
                            self.top.guard().push_back(NotificationCardInit {
                                countdown: false,
                                notification: n.clone(),
                            });
                            info!("Moved notification {} from rest to top", id);
                        }
                    }
                }
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
            Self::Input::TimedOut(_notification_id) => { /* noop  */ }
        }
    }

    fn post_view(&mut self, widgets: &mut Self::Widgets, _sender: ComponentSender<Self>) {}
}
