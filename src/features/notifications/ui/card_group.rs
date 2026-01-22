use std::collections::HashMap;
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
    rest_index: HashMap<u64, usize>,
    title: Arc<str>,
    top: FactoryVecDeque<NotificationCard>,
    top_index: HashMap<u64, usize>,
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

    fn rebuild_index(
        target: &FactoryVecDeque<NotificationCard>,
        index_map: &mut HashMap<u64, usize>,
    ) {
        index_map.clear();
        for (i, el) in target.iter().enumerate() {
            index_map.insert(el.get_id(), i);
        }
    }

    /// Process updates for a single list (top or rest) with batched guard operations
    fn process_list_update(
        target: &mut FactoryVecDeque<NotificationCard>,
        index_map: &mut HashMap<u64, usize>,
        items: &[(Notification, ItemLifecycle)],
        group_id: &Arc<str>,
    ) {
        let known_ids: Vec<u64> = items.iter().map(|(n, _)| n.id).collect();

        // Find items to remove
        let to_remove: Vec<u64> = index_map
            .keys()
            .filter(|id| !known_ids.contains(id))
            .copied()
            .collect();

        // Find items to add
        let to_add: Vec<&(Notification, ItemLifecycle)> = items
            .iter()
            .filter(|(n, _)| !index_map.contains_key(&n.id))
            .collect();

        // Single guard scope for all mutations
        {
            let mut guard = target.guard();

            // Remove items (reverse order to maintain indices)
            let mut remove_indices: Vec<(u64, usize)> = to_remove
                .iter()
                .filter_map(|id| index_map.get(id).map(|idx| (*id, *idx)))
                .collect();
            remove_indices.sort_by(|a, b| b.1.cmp(&a.1));

            for (_id, index) in &remove_indices {
                guard.remove(*index);
            }

            // Add new items
            for (notif, phase) in &to_add {
                guard.push_back(NotificationCardInit {
                    description: notif.description.clone(),
                    group_id: Some(group_id.clone()),
                    hidden: phase.is_hidden(),
                    lifecycle: Some(phase.clone()),
                    id: notif.id,
                    ttl: notif.ttl,
                    title: notif.title.clone(),
                });
            }
        }

        // Rebuild index after mutations
        Self::rebuild_index(target, index_map);

        // Send updates to existing cards
        for (n, l) in items {
            if let Some(&index) = index_map.get(&n.id) {
                target.send(
                    index,
                    NotificationCardInput::UpdateData {
                        title: n.title.clone(),
                        description: n.description.clone(),
                        lifecycle: Some(l.clone()),
                    },
                );
            }
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

        let mut top_index = HashMap::new();
        let mut rest_index = HashMap::new();

        // Initial population with batched guard
        {
            let mut guard = top.guard();
            for (i, (notif, phase)) in value.top.iter().enumerate() {
                guard.push_back(NotificationCardInit {
                    description: notif.description.clone(),
                    group_id: Some(value.id.clone()),
                    hidden: phase.is_hidden(),
                    lifecycle: Some(phase.clone()),
                    id: notif.id,
                    ttl: notif.ttl,
                    title: notif.title.clone(),
                });
                top_index.insert(notif.id, i);
            }
        }

        {
            let mut guard = rest.guard();
            for (i, (notif, phase)) in value.rest.iter().enumerate() {
                guard.push_back(NotificationCardInit {
                    description: notif.description.clone(),
                    group_id: Some(value.id.clone()),
                    hidden: phase.is_hidden(),
                    lifecycle: Some(phase.clone()),
                    id: notif.id,
                    ttl: notif.ttl,
                    title: notif.title.clone(),
                });
                rest_index.insert(notif.id, i);
            }
        }

        Self {
            id: value.id,
            expanded: value.expanded,
            lifecycle: value.lifecycle.clone(),
            title: value.title,
            top,
            top_index,
            rest,
            rest_index,
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
                Self::process_list_update(&mut self.top, &mut self.top_index, &top, &self.id);
                Self::process_list_update(&mut self.rest, &mut self.rest_index, &rest, &self.id);

                if self.rest.is_empty() {
                    send_or_log(&sender, Self::Output::Collapse(self.id.clone()));
                }
            }
        }
    }

    fn post_view(&mut self, widgets: &mut Self::Widgets, _sender: ComponentSender<Self>) {}
}
