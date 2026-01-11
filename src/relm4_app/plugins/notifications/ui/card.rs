use adw::prelude::*;
use relm4::factory::FactoryHashMap;
use relm4::gtk;
use relm4::prelude::*;
use std::sync::Arc;

use super::super::types::NotificationDisplay;
use super::card_action::{NotificationCardActionInit, NotificationCardActionOutput};

use super::card_action::NotificationCardAction;
use super::countdown_bar::{CountdownBar, CountdownBarInit, CountdownBarInput, CountdownBarOutput};
use super::icon::{Icon, IconInit, IconInput};

#[derive(Debug, Clone)]
pub struct NotificationContentUpdate {
    pub notification: Arc<NotificationDisplay>,
}

pub struct NotificationCard {
    actions: FactoryHashMap<NotificationCardActionInit, NotificationCardAction>,
    countdown_bar: Option<Controller<CountdownBar>>,
    icon: Controller<Icon>,
    notification: Arc<NotificationDisplay>,
}

pub struct NotificationCardInit {
    pub countdown: bool,
    pub notification: Arc<NotificationDisplay>,
}

#[derive(Debug, Clone)]
pub enum NotificationCardInput {
    ActionClick(String),
    CardClick,
    CloseClick,
    Content(NotificationContentUpdate),
    CountdownContinue,
    CountdownElapsed,
    CountdownPause,
    CountdownStart,
    CountdownStop,
}

#[derive(Debug, Clone)]
pub enum NotificationCardOutput {
    ActionClick(u64, String),
    CardClick(u64),
    Close(u64),
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

impl NotificationCard {}

#[relm4::factory(pub)]
impl FactoryComponent for NotificationCard {
    type Init = NotificationCardInit;
    type Input = NotificationCardInput;
    type Output = NotificationCardOutput;
    type CommandOutput = ();
    type ParentWidget = gtk::Box;
    type Index = u64;

    view! {
      gtk::Box {
        set_orientation: gtk::Orientation::Vertical,
        set_css_classes: &["card", "notification-card"],

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
              set_markup: &self.notification.title,
              set_wrap: true,
              set_wrap_mode: gtk::pango::WrapMode::WordChar,
              set_xalign: 0.0,
            },

            gtk::Label {
              set_css_classes: &["dim-label"],
              #[watch]
              set_markup: &self.notification.description,
              set_wrap: true,
              set_wrap_mode: gtk::pango::WrapMode::WordChar,
              set_xalign: 0.0,
            },
          },

          gtk::Box {
            set_hexpand: true,
          },

          gtk::Button {
            set_icon_name: "window-close-symbolic",
            set_css_classes: &["flat", "circular", "notification-close"],
            set_valign: gtk::Align::Start,
            set_halign: gtk::Align::End,
            connect_clicked => Self::Input::CloseClick,
          }
        },

        #[local_ref]
        countdown -> gtk::Box {
          set_hexpand: true,
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
    }

    fn init_model(init: Self::Init, _index: &Self::Index, sender: FactorySender<Self>) -> Self {
        let countdown_bar = match init.notification.ttl {
            Some(ttl) => Some(
                CountdownBar::builder()
                    .launch(CountdownBarInit { ttl })
                    .forward(sender.input_sender(), transform_countdown_events),
            ),
            _ => None,
        };
        let icon = Icon::builder()
            .launch(IconInit {
                icon: init.notification.icon.clone(),
            })
            .detach();

        let actions = FactoryHashMap::builder()
            .launch(gtk::Box::default())
            .forward(sender.input_sender(), transform_action_outputs);

        Self {
            actions: actions,
            icon,
            notification: init.notification,
            countdown_bar,
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
        let countdown = match &self.countdown_bar {
            Some(countdown) => countdown.widget(),
            None => &gtk::Box::default(),
        };
        let widgets = view_output!();
        widgets
    }

    fn update(&mut self, msg: Self::Input, sender: FactorySender<Self>) {
        match msg {
            NotificationCardInput::ActionClick(action) => {
                sender.output(Self::Output::ActionClick(self.notification.id, action));
            }
            NotificationCardInput::CardClick => {
                sender.output(Self::Output::CardClick(self.notification.id));
            }
            NotificationCardInput::CloseClick => {
                sender.output(Self::Output::Close(self.notification.id));
            }
            NotificationCardInput::Content(content) => {
                self.notification = content.notification;
                self.icon
                    .sender()
                    .send(IconInput::Icon(self.notification.icon.clone()));
            }
            NotificationCardInput::CountdownElapsed => {
                sender.output(Self::Output::TimedOut(self.notification.id));
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
}
