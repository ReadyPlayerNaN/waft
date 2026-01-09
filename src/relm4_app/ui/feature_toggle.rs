use crate::classnames;

use gtk::prelude::*;
use relm4::prelude::*;

use relm4::{ComponentParts, ComponentSender, SimpleComponent};

#[derive(Debug, Clone)]
pub struct FeatureToggleProps {
    pub active: bool,
    pub busy: bool,
    pub details: Option<String>,
    pub icon: String,
    pub title: String,
}

/// Internal messages for the component.
#[derive(Debug, Clone)]
pub enum Input {
    Active(bool),
    Busy(bool),
    Click,
    Details(Option<String>),
    #[allow(dead_code)]
    Icon(String),
    #[allow(dead_code)]
    Title(String),
}

/// Internal messages for the component.
#[derive(Debug, Clone)]
pub enum Output {
    Activate,
    Deactivate,
}

pub struct FeatureToggleComponent {
    active: bool,
    busy: bool,
    details: Option<String>,
    icon: String,
    title: String,
}

#[relm4::component(pub)]
impl SimpleComponent for FeatureToggleComponent {
    type Init = FeatureToggleProps;
    type Input = Input;
    type Output = Output;

    view! {
      gtk::Button {
        #[watch]
        set_css_classes: &classnames![
            "feature-toggle" => true,
            "active" => model.active,
            "busy" => model.busy,
        ],
        set_hexpand: true,
        connect_clicked => Self::Input::Click,
        gtk::Box {
          set_orientation: gtk::Orientation::Horizontal,
          set_spacing: 12,
          set_valign: gtk::Align::Center,
          gtk::Image {
            #[watch]
            set_icon_name: Some(&model.icon),
            set_pixel_size: 24,
          },
          gtk::Box {
            set_orientation: gtk::Orientation::Vertical,
            set_valign: gtk::Align::Center,
            set_spacing: 2,
            set_css_classes: &["text-content"],
            gtk::Label {
              #[watch]
              set_label: &model.title,
              set_css_classes: &["heading", "title"],
            },
            gtk::Revealer {
              set_transition_type: gtk::RevealerTransitionType::SlideDown,
              #[watch]
              set_reveal_child: model.details.is_some(),

              gtk::Label {
                #[watch]
                set_label: &model.details.clone().unwrap_or("".into()),
                set_css_classes: &["dim-label", "caption"],
                set_xalign: 0.0,
              }
            }
          }
        }
      }
    }

    fn init(
        init: FeatureToggleProps,
        root: Self::Root,
        _sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        let model = Self {
            active: init.active,
            busy: init.busy,
            details: init.details,
            icon: init.icon,
            title: init.title,
        };
        let widgets = view_output!();
        ComponentParts { model, widgets }
    }

    fn update(&mut self, msg: Self::Input, sender: ComponentSender<Self>) {
        println!("Update called, {:?}", msg);
        match msg {
            Input::Active(active) => self.active = active,
            Input::Busy(busy) => self.busy = busy,
            Input::Details(details) => self.details = details,
            Input::Icon(icon) => self.icon = icon,
            Input::Title(title) => self.title = title,
            Input::Click => {
                if self.active {
                    sender.output(Self::Output::Deactivate).unwrap();
                } else {
                    sender.output(Self::Output::Activate).unwrap();
                }
            }
        }
    }
}
