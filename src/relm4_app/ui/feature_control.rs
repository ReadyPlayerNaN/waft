use gtk::prelude::*;
use relm4::prelude::*;

use relm4::{ComponentParts, ComponentSender, SimpleComponent};

use crate::classnames;

#[derive(Debug, Clone)]
pub struct FeatureControlInit {
    pub active: bool,
    pub busy: bool,
    pub details: Option<String>,
    pub expanded: bool,
    pub icon: String,
    pub title: String,
}

/// Internal messages for the component.
#[derive(Debug, Clone)]
pub enum FeatureControlInput {
    Active(bool),
    Busy(bool),
    ToggleClick,
    ExpanderClick,
    Details(Option<String>),
    #[allow(dead_code)]
    Icon(String),
    #[allow(dead_code)]
    Title(String),
}

/// Internal messages for the component.
#[derive(Debug, Clone)]
pub enum FeatureControlOutput {
    Activate,
    Collapse,
    Deactivate,
    Expand,
}

pub struct FeatureControlComponent {
    active: bool,
    busy: bool,
    details: Option<String>,
    pub expanded: bool,
    icon: String,
    title: String,
}

#[relm4::component(pub)]
impl SimpleComponent for FeatureControlComponent {
    type Init = FeatureControlInit;
    type Input = FeatureControlInput;
    type Output = FeatureControlOutput;

    view! {
      gtk::Box {
        set_orientation: gtk::Orientation::Horizontal,
        set_css_classes: &classnames![
            "feature-control" => true,
            "active" => model.active,
            "busy" => model.busy,
        ],

        gtk::Button {
          #[watch]
          set_css_classes: &["toggle"],
          connect_clicked => Self::Input::ToggleClick,
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
              #[watch]
              set_spacing: match model.details.is_some() {true => 2, false => 0},
              set_css_classes: &["text-content"],
              gtk::Label {
                #[watch]
                set_label: &model.title,
                set_css_classes: &["heading", "title"],
              },
              gtk::Label {
                #[watch]
                set_label: &model.details.clone().unwrap_or("".into()),
                set_visible: model.details.is_some(),
                set_css_classes: &["dim-label", "caption"],
              }
            }
          }
        },

        gtk::Button {
          #[watch]
          set_css_classes: &["expander"],
          connect_clicked => Self::Input::ExpanderClick,
          gtk::Box {
            set_orientation: gtk::Orientation::Horizontal,
            set_spacing: 12,
            set_valign: gtk::Align::Center,
            gtk::Image {
              #[watch]
              set_icon_name: match &model.expanded {
                true => "pan-down-symbolic".into(),
                false => "pan-end-symbolic".into()
              },
              set_pixel_size: 24,
            }
          }
        }
      }
    }

    fn init(
        init: FeatureControlInit,
        root: Self::Root,
        _sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        let model = Self {
            active: init.active,
            busy: init.busy,
            details: init.details,
            expanded: init.expanded,
            icon: init.icon,
            title: init.title,
        };
        let widgets = view_output!();
        ComponentParts { model, widgets }
    }

    fn update(&mut self, msg: Self::Input, sender: ComponentSender<Self>) {
        match msg {
            FeatureControlInput::Active(active) => self.active = active,
            FeatureControlInput::Busy(busy) => self.busy = busy,
            FeatureControlInput::Details(details) => self.details = details,
            FeatureControlInput::Icon(icon) => self.icon = icon,
            FeatureControlInput::Title(title) => self.title = title,
            FeatureControlInput::ToggleClick => {
                if self.active {
                    sender.output(Self::Output::Deactivate).unwrap();
                } else {
                    sender.output(Self::Output::Activate).unwrap();
                }
            }
            FeatureControlInput::ExpanderClick => {
                if self.expanded {
                    sender.output(Self::Output::Collapse).unwrap();
                } else {
                    sender.output(Self::Output::Expand).unwrap();
                }
            }
        }
    }
}
