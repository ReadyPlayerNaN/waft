use gtk::prelude::*;
use relm4::prelude::*;
use relm4::{ComponentParts, ComponentSender, SimpleComponent, gtk};

use crate::ui::feature_toggle::{
    FeatureToggleComponent, FeatureToggleProps, Input as FeatureToggleInput,
    Output as FeatureToggleOutput,
};

use super::values::Status;

#[derive(Debug, Clone)]
pub enum Input {
    Busy(bool),
    ClickActivate,
    ClickDeactivate,
    Error(String),
    Status(Status),
}

#[derive(Debug, Clone)]
pub enum Output {
    Activate,
    Deactivate,
}

pub struct FeatureToggle {
    toggle: Controller<FeatureToggleComponent>,
    last_error: Option<String>,
}

pub struct Init {
    pub active: bool,
    pub busy: bool,
    pub next_transition: Option<String>,
}

fn parse_interaction(response: FeatureToggleOutput) -> Input {
    match response {
        FeatureToggleOutput::Activate => Input::ClickActivate,
        FeatureToggleOutput::Deactivate => Input::ClickDeactivate,
    }
}

#[relm4::component(pub)]
impl SimpleComponent for FeatureToggle {
    type Init = Init;
    type Input = Input;
    type Output = Output;

    view! {
      gtk::Box {
        append: model.toggle.widget(),
      }
    }

    fn init(
        init: Self::Init,
        root: Self::Root,
        sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        let toggle = FeatureToggleComponent::builder()
            .launch(FeatureToggleProps {
                title: "Night light".into(),
                icon: "night-light-symbolic".into(),
                details: init.next_transition,
                active: init.active,
                busy: init.busy,
            })
            .forward(sender.input_sender(), parse_interaction);

        let model = FeatureToggle {
            toggle,
            last_error: None,
        };
        let widgets = view_output!();
        ComponentParts { model, widgets }
    }

    fn update(&mut self, msg: Self::Input, sender: ComponentSender<Self>) {
        match msg {
            Self::Input::ClickActivate => {
                sender.output(self::Output::Activate).unwrap();
            }
            Self::Input::ClickDeactivate => {
                sender.output(self::Output::Deactivate).unwrap();
            }
            Self::Input::Status(status) => {
                let sender = self.toggle.sender();
                sender
                    .send(FeatureToggleInput::Active(status.active))
                    .unwrap();
                sender
                    .send(FeatureToggleInput::Details(
                        status
                            .next_transition_text
                            .map(|text| format!("Until: {}", text)),
                    ))
                    .unwrap();
                sender.send(FeatureToggleInput::Busy(false)).unwrap();
            }
            Self::Input::Busy(b) => {
                let sender = self.toggle.sender();
                sender.send(FeatureToggleInput::Busy(b)).unwrap();
            }
            Self::Input::Error(e) => {
                self.last_error = Some(e);
            }
        }
    }
}
