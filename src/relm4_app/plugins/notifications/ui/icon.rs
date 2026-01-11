use relm4::{ComponentParts, ComponentSender, SimpleComponent, gtk};
use std::sync::Arc;

use adw::prelude::*;

use super::super::types::NotificationIcon;

struct IconProps {
    icon_name: Option<Arc<str>>,
    icon_texture: Option<gtk::gdk::Texture>,
}

pub struct Icon {
    icon_name: Option<Arc<str>>,
    icon_texture: Option<gtk::gdk::Texture>,
}

pub struct IconInit {
    pub icon: NotificationIcon,
}

#[derive(Debug, Clone)]
pub enum IconInput {
    Icon(NotificationIcon),
}

impl Icon {
    fn parse_icon(icon: NotificationIcon) -> IconProps {
        match icon {
            NotificationIcon::Themed(name) => IconProps {
                icon_name: Some(name),
                icon_texture: None,
            },
            NotificationIcon::FilePath(path) => {
                if let Ok(tex) = gtk::gdk::Texture::from_filename(path.as_ref()) {
                    IconProps {
                        icon_name: None,
                        icon_texture: Some(tex),
                    }
                } else {
                    IconProps {
                        icon_name: Some("dialog-information-symbolic".into()),
                        icon_texture: None,
                    }
                }
            }
            NotificationIcon::Bytes(_b) => IconProps {
                // TODO: Implement icon parsing from bytes
                icon_name: Some("dialog-information-symbolic".into()),
                icon_texture: None,
            },
        }
    }
}

#[relm4::component(pub)]
impl SimpleComponent for Icon {
    type Init = IconInit;
    type Input = IconInput;
    type Output = ();

    view! {
      gtk::Image {
        set_pixel_size: 32,
        set_valign: gtk::Align::Start,
        set_icon_name: model.icon_name.as_deref(),
        set_paintable: model.icon_texture.as_ref(),
      }
    }

    fn init(
        init: Self::Init,
        root: Self::Root,
        _sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        let props = Self::parse_icon(init.icon);
        let model = Icon {
            icon_name: props.icon_name,
            icon_texture: props.icon_texture,
        };
        let widgets = view_output!();
        ComponentParts { model, widgets }
    }

    fn update(&mut self, msg: Self::Input, _sender: ComponentSender<Self>) {
        match msg {
            Self::Input::Icon(icon) => {
                let props = Self::parse_icon(icon);
                self.icon_name = props.icon_name;
                self.icon_texture = props.icon_texture;
            }
        }
    }
}
