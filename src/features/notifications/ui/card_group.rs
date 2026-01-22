use std::sync::Arc;

use adw::prelude::*;
use relm4::factory::FactoryVecDeque;
use relm4::gtk;
use relm4::prelude::*;

use crate::ui::events::send_or_log;

use super::super::store::{ItemLifecycle, Notification};
use super::card::{NotificationCard, NotificationCardInit, NotificationCardInput, NotificationCardOutput};

pub struct NotificationCardGroup {
    expanded: bool,
    id: Arc<str>,
    pub lifecycle: ItemLifecycle,
    rest: FactoryVecDeque<NotificationCard>,
    title: Arc<str>,
    top: FactoryVecDeque<NotificationCard>,
}

pub struct NotificationCardGroupInit {
    pub expanded: bool,
    pub lifecycle: ItemLifecycle,
    pub id: Arc<str>,
    pub title: Arc<str>,
    pub top: Vec<(Notification, ItemLifecycle)>,
    pub rest: Vec<(Notification, ItemLifecycle)>,
}

#[derive(Debug, Clone)]
pub enum NotificationCardGroupInput {
    ActionClick(u64, String),
    Expand(bool),
    ExpandClick,
    CardClick(u64),
    TimedOut(u64),
    UpdateGroup {
        top: Vec<(Notification, ItemLifecycle)>,
        rest: Vec<(Notification, ItemLifecycle)>,
    },
}

#[derive(Debug, Clone)]
pub enum NotificationCardGroupOutput {
    ActionClick(u64, String),
    CardClick(u64),
    Collapse(Arc<str>),
    Expand(Arc<str>),
}

fn transform_notification_card_outputs(msg: NotificationCardOutput) -> NotificationCardGroupInput {
    match msg {
        NotificationCardOutput::ActionClick(id, action) => {
            NotificationCardGroupInput::ActionClick(id, action)
        }
        NotificationCardOutput::CardClick(id) => NotificationCardGroupInput::CardClick(id),
        NotificationCardOutput::TimedOut(id) => NotificationCardGroupInput::TimedOut(id),
    }
}

impl NotificationCardGroup {
    pub fn get_id(&self) -> Arc<str> {
        self.id.clone()
    }

    fn get_index(target: &FactoryVecDeque<NotificationCard>, id: u64) -> Option<usize> {
        for (i, el) in target.iter().enumerate() {
            if el.get_id() == id {
                return Some(i);
            }
        }
        None
    }

    fn ingest_notification(
        target: &mut FactoryVecDeque<NotificationCard>,
        notif: &Notification,
        phase: &ItemLifecycle,
    ) {
        match Self::get_index(target, notif.id) {
            Some(_index) => {}
            None => {
                target.guard().push_back(NotificationCardInit {
                    description: notif.description.clone(),
                    group_id: Some(notif.app_ident()),
                    hidden: phase.is_hidden(),
                    lifecycle: Some(phase.clone()),
                    id: notif.id.clone(),
                    ttl: notif.ttl,
                    title: notif.title.clone(),
                });
            }
        }
    }

    fn get_unknown_ids(
        target: &mut FactoryVecDeque<NotificationCard>,
        known_ids: Vec<u64>,
    ) -> Vec<u64> {
        let mut unknown = vec![];
        for el in target.iter() {
            let id = el.get_id();
            if !known_ids.contains(&id) {
                unknown.push(id);
            }
        }
        return unknown;
    }

    fn remove_by_id(target: &mut FactoryVecDeque<NotificationCard>, id: u64) {
        let index = Self::get_index(target, id);
        if let Some(index) = index {
            target.guard().remove(index);
        }
    }

    fn clear_unknown(
        target: &mut FactoryVecDeque<NotificationCard>,
        items: &Vec<(&Notification, &ItemLifecycle)>,
    ) {
        let known_ids = items.iter().map(|(n, _)| n.id).collect::<Vec<u64>>();
        let unknown_ids = Self::get_unknown_ids(target, known_ids);
        for id in unknown_ids.into_iter() {
            Self::remove_by_id(target, id);
        }
    }

    fn send_update_to_card(
        target: &mut FactoryVecDeque<NotificationCard>,
        notif: &Notification,
        lifecycle: &ItemLifecycle,
    ) {
        if let Some(index) = Self::get_index(target, notif.id) {
            target.send(
                index,
                NotificationCardInput::UpdateData {
                    title: notif.title.clone(),
                    description: notif.description.clone(),
                    lifecycle: Some(lifecycle.clone()),
                },
            );
        }
    }
}

#[relm4::factory(pub)]
impl FactoryComponent for NotificationCardGroup {
    type Index = relm4::factory::DynamicIndex;
    type Init = NotificationCardGroupInit;
    type Input = NotificationCardGroupInput;
    type Output = NotificationCardGroupOutput;
    type CommandOutput = ();
    type ParentWidget = gtk::Box;

    view! {
      gtk::Box {
        set_orientation: gtk::Orientation::Vertical,

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
    }

    fn init_model(value: Self::Init, _index: &Self::Index, sender: FactorySender<Self>) -> Self {
        let mut top = FactoryVecDeque::builder()
            .launch(gtk::Box::default())
            .forward(sender.input_sender(), transform_notification_card_outputs);

        let mut rest = FactoryVecDeque::builder()
            .launch(gtk::Box::default())
            .forward(sender.input_sender(), transform_notification_card_outputs);

        for (group, phase) in value.top.into_iter() {
            Self::ingest_notification(&mut top, &group, &phase);
        }
        for (group, phase) in value.rest.into_iter() {
            Self::ingest_notification(&mut rest, &group, &phase);
        }
        Self {
            id: value.id,
            expanded: value.expanded,
            lifecycle: value.lifecycle.clone(),
            title: value.title,
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
            Self::Input::ActionClick(notification_id, action) => {
                send_or_log(&sender, Self::Output::ActionClick(notification_id, action));
            }
            Self::Input::Expand(state) => {
                self.expanded = state;
            }
            Self::Input::ExpandClick => {
                if self.expanded {
                    send_or_log(&sender, Self::Output::Collapse(self.id.clone()));
                } else {
                    send_or_log(&sender, Self::Output::Expand(self.id.clone()));
                }
            }
            Self::Input::CardClick(notification_id) => {
                send_or_log(&sender, Self::Output::CardClick(notification_id));
            }
            Self::Input::TimedOut(_notification_id) => { /* noop  */ }
            Self::Input::UpdateGroup { top, rest } => {
                let top_refs: Vec<(&Notification, &ItemLifecycle)> =
                    top.iter().map(|(n, l)| (n, l)).collect();
                let rest_refs: Vec<(&Notification, &ItemLifecycle)> =
                    rest.iter().map(|(n, l)| (n, l)).collect();

                Self::clear_unknown(&mut self.top, &top_refs);
                for (n, l) in &top {
                    Self::ingest_notification(&mut self.top, n, l);
                    Self::send_update_to_card(&mut self.top, n, l);
                }

                Self::clear_unknown(&mut self.rest, &rest_refs);
                for (n, l) in &rest {
                    Self::ingest_notification(&mut self.rest, n, l);
                    Self::send_update_to_card(&mut self.rest, n, l);
                }

                if self.rest.is_empty() {
                    send_or_log(&sender, Self::Output::Collapse(self.id.clone()));
                }
            }
        }
    }

    fn post_view(&mut self, widgets: &mut Self::Widgets, _sender: ComponentSender<Self>) {}
}
