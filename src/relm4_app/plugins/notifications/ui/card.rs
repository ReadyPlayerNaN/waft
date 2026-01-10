use adw::prelude::*;
use relm4::factory::FactoryHashMap;
use relm4::gtk;
use relm4::prelude::*;
use std::time::SystemTime;

use super::super::types::{NotificationDisplay, NotificationUrgency};
use super::card_action::{NotificationCardActionInit, NotificationCardActionOutput};

use super::super::types::NotificationIcon;
use super::card_action::NotificationCardAction;
use super::icon::{Icon, IconInit, IconInput};
use super::progress_bar::{ProgressBar, ProgressBarInit, ProgressBarInput};

#[derive(Debug, Clone)]
pub struct NotificationContentUpdate {
    pub title: String,
    pub description: String,
    pub icon: NotificationIcon,
    pub created_at: Option<SystemTime>,
}

pub struct NotificationCard {
    id: u64,
    actions: FactoryHashMap<NotificationCardActionInit, NotificationCardAction>,
    created_at: SystemTime, // @TODO: Display created at time
    description: String,
    icon: Controller<Icon>,
    progress_bar: Controller<ProgressBar>,
    title: String,
    urgency: NotificationUrgency,
}

pub type NotificationCardInit = NotificationDisplay;

#[derive(Debug, Clone)]
pub enum NotificationCardInput {
    ActionClick(String),
    CardClick,
    CloseClick,
    Content(NotificationContentUpdate),
    Icon(NotificationIcon),
    Progress(f32),
}

#[derive(Debug, Clone)]
pub enum NotificationCardOutput {
    ActionClick(u64, String),
    CardClick(u64),
    Close(u64),
}

fn transform_action_outputs(msg: NotificationCardActionOutput) -> NotificationCardInput {
    match msg {
        NotificationCardActionOutput::Click(key) => NotificationCardInput::ActionClick(key),
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

          gtk::Button {
            set_icon_name: "window-close-symbolic",
            set_css_classes: &["flat", "circular", "notification-close"],
            set_valign: gtk::Align::Start,
            set_halign: gtk::Align::End,
            connect_clicked => Self::Input::CloseClick,
          }
        },

        append: self.progress_bar.widget(),

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
        let progress_bar = ProgressBar::builder()
            .launch(ProgressBarInit {
                progress: init.progress,
                visible: true,
            })
            .detach();
        let icon = Icon::builder()
            .launch(IconInit { icon: init.icon })
            .detach();

        let actions = FactoryHashMap::builder()
            .launch(gtk::Box::default())
            .forward(sender.input_sender(), transform_action_outputs);

        Self {
            id: init.id,
            actions: actions,
            created_at: init.created_at,
            description: init.description,
            icon,
            progress_bar,
            title: init.title,
            urgency: init.urgency,
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
        let widgets = view_output!();
        widgets
    }

    fn update(&mut self, msg: Self::Input, sender: FactorySender<Self>) {
        match msg {
            NotificationCardInput::CardClick => {
                sender.output(Self::Output::CardClick(self.id.clone()));
            }
            NotificationCardInput::CloseClick => {
                sender.output(Self::Output::Close(self.id));
            }
            NotificationCardInput::Content(content) => {
                self.title = content.title;
                self.description = content.description;
                self.icon.sender().send(IconInput::Icon(content.icon));
            }
            NotificationCardInput::Progress(progress) => {
                self.progress_bar
                    .sender()
                    .send(ProgressBarInput::Progress(progress));
            }
            NotificationCardInput::Icon(icon) => {
                self.icon.sender().send(IconInput::Icon(icon));
            }
            NotificationCardInput::ActionClick(action) => {
                sender.output(Self::Output::ActionClick(self.id, action));
            }
        }
    }
}
