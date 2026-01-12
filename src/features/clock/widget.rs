use log::warn;

use gtk::glib::{DateTime, GString};
use gtk::prelude::*;
use relm4::{ComponentParts, ComponentSender, SimpleComponent, gtk};

pub struct ClockWidget {
    date: String,
    time: String,
}

pub struct ClockInit {
    pub datetime: DateTime,
}

#[derive(Debug, Clone)]
pub enum ClockInput {
    Click,
    Tick(DateTime),
}

#[derive(Debug, Clone)]
pub enum ClockOutput {
    Click,
}

impl ClockWidget {
    fn format_datetime_str(d: &DateTime, format: &str) -> GString {
        match d.format(format) {
            Ok(s) => s,
            Err(_e) => {
                warn!("Failed to format datetime with format: {}", format);
                "".into()
            }
        }
    }

    fn format_date(d: &DateTime) -> String {
        Self::format_datetime_str(d, "%a, %d %b %Y").to_string()
    }

    fn format_time(d: &DateTime) -> String {
        Self::format_datetime_str(d, "%H:%M").to_string()
    }
}

#[relm4::component(pub)]
impl SimpleComponent for ClockWidget {
    type Init = ClockInit;
    type Input = ClockInput;
    type Output = ClockOutput;

    view! {
      gtk::Button {
        connect_clicked => Self::Input::Click,
        set_css_classes: &["clock-btn"],
        gtk::Box {
          set_orientation: gtk::Orientation::Vertical,
          set_spacing: 2,
          set_css_classes: &["clock-container"],

          gtk::Label {
            set_label: &model.date,
            set_xalign: 0.0,
            set_css_classes: &["title-3", "dim-label", "clock-date"],
          },

          gtk::Label {
            #[watch]
            set_label: &model.time,
            set_xalign: 0.0,
            set_css_classes: &["title-1", "clock-time"]
          }
        }
      }
    }

    fn init(
        init: Self::Init,
        root: Self::Root,
        _sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        let model = ClockWidget {
            date: Self::format_date(&init.datetime),
            time: Self::format_time(&init.datetime),
        };
        let widgets = view_output!();
        ComponentParts { model, widgets }
    }

    fn update(&mut self, msg: Self::Input, sender: ComponentSender<Self>) {
        match msg {
            Self::Input::Tick(datetime) => {
                self.date = Self::format_date(&datetime);
                self.time = Self::format_time(&datetime);
            }
            Self::Input::Click => match sender.output(Self::Output::Click) {
                Ok(_) => {}
                Err(err) => {
                    log::error!("Failed to send output: {:?}", err);
                }
            },
        }
    }
}
