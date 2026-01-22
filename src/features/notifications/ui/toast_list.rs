use std::collections::HashMap;

use adw::prelude::*;
use relm4::factory::FactoryVecDeque;
use relm4::gtk;
use relm4::prelude::*;

use super::super::store::{ItemLifecycle, Notification, REDUCER, State};

use super::card::{
    NotificationCard, NotificationCardInit, NotificationCardInput, NotificationCardOutput,
};

pub struct ToastList {
    list: FactoryVecDeque<NotificationCard>,
    index_map: HashMap<u64, usize>,
}

#[derive(Debug, Clone)]
pub enum ToastListInput {
    ActionClick(u64, String),
    CardClick(u64),
    CardTimedOut(u64),
    StateChanged(State),
}

#[derive(Debug, Clone)]
pub enum ToastListOutput {
    ActionClick(u64, String),
    CardClick(u64),
    CardTimedOut(u64),
}

fn transform_notification_card_outputs(msg: NotificationCardOutput) -> ToastListInput {
    match msg {
        NotificationCardOutput::ActionClick(id, action) => ToastListInput::ActionClick(id, action),
        NotificationCardOutput::CardClick(id) => ToastListInput::CardClick(id),
        NotificationCardOutput::TimedOut(id) => ToastListInput::CardTimedOut(id),
    }
}

#[relm4::component(pub)]
impl SimpleComponent for ToastList {
    type Init = ();
    type Input = ToastListInput;
    type Output = ToastListOutput;
    type Widgets = ToastListWidgets;

    view! {
        #[root]
        gtk::Box {
            set_orientation: gtk::Orientation::Vertical,
            set_spacing: 8,
            #[local_ref]
            notifications_container -> gtk::Box {
                set_orientation: gtk::Orientation::Vertical,
                set_spacing: 0,
            },

            gtk::Box{},
        }
    }

    fn init(
        _value: Self::Init,
        _root: Self::Root,
        sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        let list = FactoryVecDeque::builder()
            .launch(gtk::Box::default())
            .forward(sender.input_sender(), transform_notification_card_outputs);

        let model = Self {
            list,
            index_map: HashMap::new(),
        };

        REDUCER.subscribe(sender.input_sender(), |s| {
            Self::Input::StateChanged(s.get_state().clone())
        });

        let notifications_container = model.list.widget();
        let widgets = view_output!();
        ComponentParts { model, widgets }
    }

    fn update(&mut self, msg: Self::Input, sender: ComponentSender<Self>) {
        match msg {
            Self::Input::CardTimedOut(id) => {
                let _ = sender.output(Self::Output::CardTimedOut(id));
            }
            Self::Input::ActionClick(notification_id, action) => {
                let _ = sender.output(Self::Output::ActionClick(notification_id, action));
            }
            Self::Input::CardClick(notification_id) => {
                let _ = sender.output(Self::Output::CardClick(notification_id));
            }
            Self::Input::StateChanged(state) => {
                let toasts: Vec<(Notification, ItemLifecycle)> = state
                    .get_toasts()
                    .into_iter()
                    .filter(|(_, l)| !matches!(
                        l,
                        ItemLifecycle::Dismissed | ItemLifecycle::Hidden | ItemLifecycle::Retracted
                    ))
                    .map(|(n, l)| (n.clone(), l.clone()))
                    .collect();

                // Collect IDs of toasts that should exist
                let known_ids: Vec<u64> = toasts.iter().map(|(n, _)| n.id).collect();

                // Find toasts to remove (exist in UI but not in state)
                let to_remove: Vec<u64> = self
                    .index_map
                    .keys()
                    .filter(|id| !known_ids.contains(id))
                    .copied()
                    .collect();

                // Find toasts to add (exist in state but not in UI)
                let to_add: Vec<&(Notification, ItemLifecycle)> = toasts
                    .iter()
                    .filter(|(n, _)| !self.index_map.contains_key(&n.id))
                    .collect();

                // Single guard scope for all mutations
                {
                    let mut guard = self.list.guard();

                    // Remove items not in state (reverse order to maintain indices)
                    let mut remove_indices: Vec<(u64, usize)> = to_remove
                        .iter()
                        .filter_map(|id| self.index_map.get(id).map(|idx| (*id, *idx)))
                        .collect();
                    remove_indices.sort_by(|a, b| b.1.cmp(&a.1)); // Sort descending by index

                    for (_id, index) in &remove_indices {
                        guard.remove(*index);
                    }

                    // Add new items
                    for (notif, phase) in &to_add {
                        guard.push_front(NotificationCardInit {
                            description: notif.description.clone(),
                            group_id: None,
                            hidden: true,
                            lifecycle: Some(phase.clone()),
                            id: notif.id,
                            ttl: notif.ttl,
                            title: notif.title.clone(),
                        });
                    }
                }

                // Rebuild index after all mutations
                self.rebuild_index();

                // Send updates and visibility changes (no guard needed for send)
                for (n, l) in &toasts {
                    if let Some(&index) = self.index_map.get(&n.id) {
                        self.list.send(
                            index,
                            NotificationCardInput::UpdateData {
                                title: n.title.clone(),
                                description: n.description.clone(),
                                lifecycle: Some(l.clone()),
                            },
                        );
                        self.list
                            .send(index, NotificationCardInput::VisibilityChange(!l.is_hidden()));
                    }
                }
            }
        }
    }
}

impl ToastList {
    /// Rebuild the index map from the current factory contents.
    /// Call this after any batch of mutations to the factory.
    fn rebuild_index(&mut self) {
        self.index_map.clear();
        for (i, el) in self.list.iter().enumerate() {
            self.index_map.insert(el.get_id(), i);
        }
    }
}
