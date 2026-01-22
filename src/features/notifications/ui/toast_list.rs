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

        let model = Self { list };

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
                println!(
                    "ToastList received state change {:?}",
                    state
                        .get_toasts()
                        .clone()
                        .into_iter()
                        .map(|(t, s)| (t.id, s))
                        .collect::<Vec<_>>()
                );
                let toasts: Vec<(Notification, ItemLifecycle)> = state
                    .get_toasts()
                    .into_iter()
                    .filter(|(_, l)| match l {
                        ItemLifecycle::Dismissed
                        | ItemLifecycle::Hidden
                        | ItemLifecycle::Retracted => false,
                        _ => true,
                    })
                    .map(|(n, l)| (n.clone(), l.clone()))
                    .collect();

                let toasts_refs: Vec<(&Notification, &ItemLifecycle)> =
                    toasts.iter().map(|(n, l)| (n, l)).collect();

                Self::clear_unknown(&mut self.list, &toasts_refs);
                for (n, l) in &toasts {
                    Self::ingest_notification(&mut self.list, n, l);
                    Self::send_update_to_card(&mut self.list, n, l);
                }
            }
        }
    }
}

impl ToastList {
    fn get_index(target: &FactoryVecDeque<NotificationCard>, id: u64) -> Option<usize> {
        for (i, el) in target.iter().enumerate() {
            if el.get_id() == id {
                return Some(i);
            }
        }
        None
    }

    fn toggle_by_id(target: &mut FactoryVecDeque<NotificationCard>, id: u64, visible: bool) {
        if let Some(index) = Self::get_index(target, id) {
            target.send(index, NotificationCardInput::VisibilityChange(visible));
        }
    }

    fn ingest_notification(
        target: &mut FactoryVecDeque<NotificationCard>,
        notif: &Notification,
        phase: &ItemLifecycle,
    ) {
        match Self::get_index(target, notif.id) {
            Some(_index) => {}
            None => {
                // Use push_front so promoted toasts appear at the top
                target.guard().push_front(NotificationCardInit {
                    description: notif.description.clone(),
                    group_id: None,
                    hidden: true,
                    lifecycle: Some(phase.clone()),
                    id: notif.id.clone(),
                    ttl: notif.ttl,
                    title: notif.title.clone(),
                });
                Self::toggle_by_id(target, notif.id.clone(), !phase.is_hidden());
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
