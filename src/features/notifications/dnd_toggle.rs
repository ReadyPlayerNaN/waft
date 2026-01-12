use gtk::prelude::*;
use relm4::prelude::*;
use relm4::{ComponentParts, ComponentSender, SimpleComponent, gtk};

use crate::ui::feature_toggle::{
    FeatureToggleComponent, FeatureToggleProps, Input as FeatureToggleInput,
    Output as FeatureToggleOutput,
};

#[derive(Debug, Clone)]
pub enum DoNotDisturbToggleInput {
    Active(bool),
    ClickActivate,
    ClickDeactivate,
}

#[derive(Debug, Clone)]
pub enum DoNotDisturbToggleOutput {
    Activate,
    Deactivate,
}

pub struct DoNotDisturbToggle {
    toggle: Controller<FeatureToggleComponent>,
}

pub struct DoNotDisturbToggleInit {
    pub active: bool,
    pub busy: bool,
}

fn parse_interaction(response: FeatureToggleOutput) -> DoNotDisturbToggleInput {
    match response {
        FeatureToggleOutput::Activate => DoNotDisturbToggleInput::ClickActivate,
        FeatureToggleOutput::Deactivate => DoNotDisturbToggleInput::ClickDeactivate,
    }
}

#[relm4::component(pub)]
impl SimpleComponent for DoNotDisturbToggle {
    type Init = DoNotDisturbToggleInit;
    type Input = DoNotDisturbToggleInput;
    type Output = DoNotDisturbToggleOutput;

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
                title: "Do not disturb".into(),
                icon: "notifications-disabled-symbolic".into(),
                details: None,
                active: init.active,
                busy: init.busy,
            })
            .forward(sender.input_sender(), parse_interaction);

        let model = DoNotDisturbToggle { toggle };
        let widgets = view_output!();
        ComponentParts { model, widgets }
    }

    fn update(&mut self, msg: Self::Input, sender: ComponentSender<Self>) {
        match msg {
            Self::Input::ClickActivate => {
                sender
                    .output(self::DoNotDisturbToggleOutput::Activate)
                    .unwrap();
            }
            Self::Input::ClickDeactivate => {
                sender
                    .output(self::DoNotDisturbToggleOutput::Deactivate)
                    .unwrap();
            }
            Self::Input::Active(status) => {
                let sender = self.toggle.sender();
                sender.send(FeatureToggleInput::Active(status)).unwrap();
                sender.send(FeatureToggleInput::Busy(false)).unwrap();
            }
        }
    }
}
