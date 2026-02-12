//! Checkmark widget

use gtk::prelude::*;

/// GTK4 checkmark widget using "object-select-symbolic" icon.
pub struct CheckmarkWidget {
    image: gtk::Image,
}

impl CheckmarkWidget {
    pub fn new(visible: bool) -> Self {
        let image = gtk::Image::from_icon_name("object-select-symbolic");
        image.add_css_class("checkmark");
        image.set_visible(visible);
        Self { image }
    }

    pub fn set_visible(&self, visible: bool) {
        self.image.set_visible(visible);
    }
}

impl crate::widget_base::WidgetBase for CheckmarkWidget {
    fn widget(&self) -> gtk::Widget {
        self.image.clone().upcast()
    }
}
