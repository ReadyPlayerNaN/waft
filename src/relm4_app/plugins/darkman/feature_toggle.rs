use gtk::prelude::*;
use relm4::prelude::*;
use relm4::{ComponentParts, ComponentSender, SimpleComponent, gtk};

use crate::relm4_app::ui::feature_toggle::{
    FeatureToggleComponent, FeatureToggleProps, Input as FeatureToggleInput,
    Output as FeatureToggleOutput,
};

#[derive(Debug, Clone)]
pub enum DarkmanToggleInput {
    Active(bool),
    Busy(bool),
    ClickActivate,
    ClickDeactivate,
}

#[derive(Debug, Clone)]
pub enum DarkmanToggleOutput {
    Activate,
    Deactivate,
}

pub struct DarkmanToggle {
    toggle: Controller<FeatureToggleComponent>,
}

pub struct DarkmanToggleInit {
    pub active: bool,
    pub busy: bool,
}

fn parse_interaction(response: FeatureToggleOutput) -> DarkmanToggleInput {
    match response {
        FeatureToggleOutput::Activate => DarkmanToggleInput::ClickActivate,
        FeatureToggleOutput::Deactivate => DarkmanToggleInput::ClickDeactivate,
    }
}

#[relm4::component(pub)]
impl SimpleComponent for DarkmanToggle {
    type Init = DarkmanToggleInit;
    type Input = DarkmanToggleInput;
    type Output = DarkmanToggleOutput;

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
                title: "Dark Mode".into(),
                icon: "weather-clear-night-symbolic".into(),
                details: None,
                active: init.active,
                busy: init.busy,
            })
            .forward(sender.input_sender(), parse_interaction);

        let model = DarkmanToggle { toggle };
        let widgets = view_output!();
        ComponentParts { model, widgets }
    }

    fn update(&mut self, msg: Self::Input, sender: ComponentSender<Self>) {
        match msg {
            Self::Input::ClickActivate => {
                sender.output(self::DarkmanToggleOutput::Activate).unwrap();
            }
            Self::Input::ClickDeactivate => {
                sender
                    .output(self::DarkmanToggleOutput::Deactivate)
                    .unwrap();
            }
            Self::Input::Active(status) => {
                let sender = self.toggle.sender();
                sender.send(FeatureToggleInput::Active(status)).unwrap();
                sender.send(FeatureToggleInput::Busy(false)).unwrap();
            }
            Self::Input::Busy(b) => {
                let sender = self.toggle.sender();
                sender.send(FeatureToggleInput::Busy(b)).unwrap();
            }
        }
    }
}
