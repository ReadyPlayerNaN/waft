use adw::prelude::*;
use relm4::gtk;
use relm4::prelude::*;

pub struct NotificationCardAction {
    title: String,
    key: String,
}

pub struct NotificationCardActionInit {
    pub title: String,
}

#[derive(Debug, Clone)]
pub enum NotificationCardActionInput {
    Click,
    Title(String),
}

#[derive(Debug, Clone)]
pub enum NotificationCardActionOutput {
    Click(String),
}

impl NotificationCardAction {}

#[relm4::factory(pub)]
impl FactoryComponent for NotificationCardAction {
    type Init = NotificationCardActionInit;
    type Input = NotificationCardActionInput;
    type Output = NotificationCardActionOutput;
    type CommandOutput = ();
    type Index = String;
    type ParentWidget = gtk::Box;

    view! {
        gtk::Button {
          set_css_classes: &["flat", "circular", "notification-close"],
          set_valign: gtk::Align::Start,
          set_halign: gtk::Align::End,
          connect_clicked => Self::Input::Click,
          gtk::Label {
            #[watch]
            set_label: &self.title,
          }
        }
    }

    fn init_model(value: Self::Init, index: &Self::Index, _sender: FactorySender<Self>) -> Self {
        Self {
            title: value.title,
            key: index.clone(),
        }
    }

    fn update(&mut self, msg: Self::Input, sender: FactorySender<Self>) {
        match msg {
            Self::Input::Title(title) => {
                self.title = title;
            }
            Self::Input::Click => {
                sender.output(Self::Output::Click(self.key.clone()));
            }
        }
    }
}
