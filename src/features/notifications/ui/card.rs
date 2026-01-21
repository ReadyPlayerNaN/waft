use std::sync::Arc;

use adw::prelude::*;
use log::info;
use relm4::factory::FactoryHashMap;
use relm4::gtk;
use relm4::gtk::prelude::{BoxExt, ButtonExt, GestureSingleExt, WidgetExt};
use relm4::prelude::*;

use crate::classnames;
use crate::features::notifications::store::{ItemLifecycle, NotificationOp, State, REDUCER};
use crate::features::notifications::types::NotificationIcon;

use super::card_action::{NotificationCardActionInit, NotificationCardActionOutput};

use super::card_action::NotificationCardAction;
use super::countdown_bar::{CountdownBar, CountdownBarInit, CountdownBarInput, CountdownBarOutput};
use super::icon::{Icon, IconInit};

pub struct NotificationCard {
    actions: FactoryHashMap<NotificationCardActionInit, NotificationCardAction>,
    countdown_bar: Option<Controller<CountdownBar>>,
    group_id: Option<Arc<str>>,
    icon: Controller<Icon>,
    id: u64,
    lifecycle: Option<ItemLifecycle>,
    title: Arc<str>,
    description: Arc<str>,
    hidden: bool,
    ttl: Option<u64>,
}

pub struct NotificationCardInit {
    pub description: Arc<str>,
    pub group_id: Option<Arc<str>>,
    pub hidden: bool,
    pub id: u64,
    pub lifecycle: Option<ItemLifecycle>,
    pub title: Arc<str>,
    pub ttl: Option<u64>,
}

#[derive(Debug, Clone)]
pub enum NotificationCardInput {
    ActionClick(String),
    CardClick,
    CloseClick,
    CountdownContinue,
    CountdownElapsed,
    CountdownPause,
    CountdownStart,
    CountdownStop,
    StateChanged(State),
    Toggled,
}

#[derive(Debug, Clone)]
pub enum NotificationCardOutput {
    ActionClick(u64, String),
    CardClick(u64),
    TimedOut(u64),
}

fn transform_action_outputs(msg: NotificationCardActionOutput) -> NotificationCardInput {
    match msg {
        NotificationCardActionOutput::Click(key) => NotificationCardInput::ActionClick(key),
    }
}

fn transform_countdown_events(msg: CountdownBarOutput) -> NotificationCardInput {
    match msg {
        CountdownBarOutput::Elapsed => NotificationCardInput::CountdownElapsed,
    }
}

impl NotificationCard {
    pub fn get_id(&self) -> u64 {
        self.id
    }

    pub fn get_hidden(&self) -> bool {
        self.hidden
    }

    pub fn get_phase(&self) -> &Option<ItemLifecycle> {
        &self.lifecycle
    }
}

#[relm4::factory(pub)]
impl FactoryComponent for NotificationCard {
    type Index = relm4::factory::DynamicIndex;
    type Init = NotificationCardInit;
    type Input = NotificationCardInput;
    type Output = NotificationCardOutput;
    type CommandOutput = ();
    type ParentWidget = gtk::Box;

    view! {
      gtk::Box {
        set_orientation: gtk::Orientation::Vertical,

        gtk::Revealer {
          #[watch]
          set_reveal_child: !self.hidden,
          set_transition_type: gtk::RevealerTransitionType::SlideDown,
          set_transition_duration: 200,
          set_css_classes: &classnames![
            "notification-card-revealer" => true,
            "hidden" => self.hidden,
          ],
          // connect_visible_notify => NotificationCardInput::Toggled,
          // connect_hide => NotificationCardInput::Toggled,

          #[name = "card_box"]
          gtk::Box {
            set_orientation: gtk::Orientation::Vertical,
            set_css_classes: &["card", "notification-card"],

            #[name = "header"]
            gtk::Box {
              set_orientation: gtk::Orientation::Horizontal,
              set_hexpand: true,
              set_spacing: 12,
              set_margin_start: 16,
              set_margin_end: 16,
              set_margin_top: 16,
              set_margin_bottom: 16,

              append: self.icon.widget(),

              gtk::Box {
                set_orientation: gtk::Orientation::Vertical,
                set_spacing: 8,
                set_css_classes: &["notification-content"],
                set_hexpand: true,
                set_halign: gtk::Align::Fill,

                gtk::Label {
                  #[watch]
                  set_css_classes: &["heading"],
                  #[watch]
                  set_markup: &self.title,
                  set_wrap: true,
                  set_wrap_mode: gtk::pango::WrapMode::WordChar,
                  set_xalign: 0.0,
                },

                gtk::Label {
                  set_css_classes: &["dim-label"],
                  #[watch]
                  set_markup: &self.description,
                  set_wrap: true,
                  set_wrap_mode: gtk::pango::WrapMode::WordChar,
                  set_xalign: 0.0,
                },
              },

              gtk::Box {
                set_hexpand: true,
              },

              #[name = "close_btn"]
              gtk::Button {
                set_icon_name: "window-close-symbolic",
                set_css_classes: &["flat", "circular", "notification-close"],
                set_valign: gtk::Align::Start,
                set_halign: gtk::Align::End,
                // connect_clicked => Self::Input::CloseClick,
              }
            },



            gtk::Box {
              set_orientation: gtk::Orientation::Vertical,
              set_css_classes: &["notification-actions-container"],
              set_margin_top: 8,

              gtk::Revealer {
                #[watch]
                set_reveal_child: self.actions.len() > 0,
                set_transition_type: gtk::RevealerTransitionType::SlideDown,

                gtk::Separator {
                  set_orientation: gtk::Orientation::Horizontal,
                  set_css_classes: &["notification-separator"],
                },

                #[local_ref]
                actions_box -> gtk::Box {
                  set_orientation: gtk::Orientation::Horizontal,
                  set_spacing: 6,
                  set_margin_start: 12,
                  set_margin_end: 12,
                  set_margin_bottom: 8,
                  set_margin_top: 8,
                }
              }
            }
          }
        },

        gtk::Box{
          set_height_request: 0,
        },
      }
    }

    fn init_model(init: Self::Init, _index: &Self::Index, sender: FactorySender<Self>) -> Self {
        let countdown_bar = match init.ttl {
            Some(ttl) => Some(
                CountdownBar::builder()
                    .launch(CountdownBarInit { ttl })
                    .forward(sender.input_sender(), transform_countdown_events),
            ),
            _ => None,
        };
        let icon = Icon::builder()
            .launch(IconInit {
                // icon: init.notification.icon.clone(),
                icon: NotificationIcon::Themed(Arc::from("xxx")),
            })
            .detach();

        let actions = FactoryHashMap::builder()
            .launch(gtk::Box::default())
            .forward(sender.input_sender(), transform_action_outputs);
        REDUCER.subscribe(sender.input_sender(), |s| {
            Self::Input::StateChanged(s.get_state().clone())
        });

        Self {
            actions: actions,
            countdown_bar,
            description: init.description,
            group_id: init.group_id,
            hidden: init.hidden,
            icon,
            id: init.id,
            lifecycle: init.lifecycle,
            title: init.title,
            ttl: init.ttl,
        }
    }

    fn init_widgets(
        &mut self,
        _index: &Self::Index,
        root: Self::Root,
        _returned_widget: &<Self::ParentWidget as relm4::factory::FactoryView>::ReturnedWidget,
        sender: FactorySender<Self>,
    ) -> Self::Widgets {
        let actions_box = self.actions.widget();

        // Right mouse button click anywhere on the card should close it,
        // same as clicking the close button.
        let input = sender.input_sender().clone();
        let right_click = gtk::GestureClick::new();
        right_click.set_button(3);
        right_click.connect_pressed(move |_gesture, _n_press, _x, _y| {
            let _ = input.send(NotificationCardInput::CloseClick);
        });
        root.add_controller(right_click);

        let widgets = view_output!();

        // Close button must not panic if the component runtime is already shut down.
        let input = sender.input_sender().clone();
        widgets.close_btn.connect_clicked(move |_| {
            let _ = input.send(NotificationCardInput::CloseClick);
        });

        // Only insert the countdown widget when it exists (no placeholder widgets).
        //
        // Important: `header` is inside `card_box`, not a direct child of `root`.
        if let Some(countdown) = &self.countdown_bar {
            widgets
                .card_box
                .insert_child_after(countdown.widget(), Some(&widgets.header));
        }

        widgets
    }

    fn update(&mut self, msg: Self::Input, sender: FactorySender<Self>) {
        match msg {
            Self::Input::StateChanged(state) => {
                if let Some(n) = state.get_notification(&self.id) {
                    self.title = n.title.clone();
                    self.description = n.description.clone();
                    self.lifecycle = state
                        .get_notification_lifecycle(&self.group_id, &self.id)
                        .map(|l| l.clone());
                    self.hidden = self
                        .lifecycle
                        .as_ref()
                        .map(|l| l.is_hidden())
                        .unwrap_or(true);
                }
            }
            Self::Input::Toggled => {}
            NotificationCardInput::ActionClick(action) => {
                sender.output(Self::Output::ActionClick(self.id, action));
            }
            NotificationCardInput::CardClick => {
                sender.output(Self::Output::CardClick(self.id));
            }
            NotificationCardInput::CloseClick => {
                // sender.output(Self::Output::Close(self.id));
                REDUCER.emit(NotificationOp::NotificationDismiss(self.id));
            }
            NotificationCardInput::CountdownElapsed => {
                sender.output(Self::Output::TimedOut(self.id));
            }
            NotificationCardInput::CountdownContinue => match &self.countdown_bar {
                Some(countdown) => {
                    countdown.sender().emit(CountdownBarInput::Continue);
                }
                None => {}
            },
            NotificationCardInput::CountdownPause => match &self.countdown_bar {
                Some(countdown) => {
                    countdown.sender().emit(CountdownBarInput::Pause);
                }
                None => {}
            },
            NotificationCardInput::CountdownStart => match &self.countdown_bar {
                Some(countdown) => {
                    countdown.sender().emit(CountdownBarInput::Start);
                }
                None => {}
            },
            NotificationCardInput::CountdownStop => match &self.countdown_bar {
                Some(countdown) => {
                    countdown.sender().emit(CountdownBarInput::Stop);
                }
                None => {}
            },
        };
    }

    fn shutdown(&mut self, _widgets: &mut Self::Widgets, _output: relm4::Sender<Self::Output>) {}
}
