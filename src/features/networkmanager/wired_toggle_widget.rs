#[allow(dead_code)]
use gtk::prelude::*;
use std::cell::RefCell;
use std::rc::Rc;

#[derive(Clone)]
pub struct WiredToggleWidget {
    inner: Rc<WiredToggleWidgetInner>,
}

struct WiredToggleWidgetInner {
    root: gtk::Box,
    interface_name: String,
    enabled: RefCell<bool>,
    carrier: RefCell<bool>,
    device_state: RefCell<u32>,
    icon: gtk::Image,
    details_label: gtk::Label,
    switch: gtk::Switch,
}

impl WiredToggleWidget {
    pub fn new(interface_name: String) -> Self {
        let root = gtk::Box::builder()
            .orientation(gtk::Orientation::Horizontal)
            .spacing(12)
            .hexpand(true)
            .build();

        let icon = gtk::Image::builder()
            .icon_name("network-wired-symbolic")
            .pixel_size(24)
            .build();

        let labels_box = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .hexpand(true)
            .build();

        let title_label = gtk::Label::builder()
            .label(&format!("Wired ({})", interface_name))
            .halign(gtk::Align::Start)
            .build();

        let details_label = gtk::Label::builder()
            .label("Disabled")
            .halign(gtk::Align::Start)
            .build();
        details_label.add_css_class("dim-label");
        details_label.add_css_class("caption");

        let switch = gtk::Switch::builder()
            .valign(gtk::Align::Center)
            .build();

        labels_box.append(&title_label);
        labels_box.append(&details_label);

        root.append(&icon);
        root.append(&labels_box);
        root.append(&switch);

        Self {
            inner: Rc::new(WiredToggleWidgetInner {
                root,
                interface_name,
                enabled: RefCell::new(false),
                carrier: RefCell::new(false),
                device_state: RefCell::new(0),
                icon,
                details_label,
                switch,
            }),
        }
    }

    pub fn widget(&self) -> gtk::Widget {
        self.inner.root.clone().upcast()
    }

    pub fn set_enabled(&self, enabled: bool) {
        *self.inner.enabled.borrow_mut() = enabled;
        self.update_ui();
    }

    pub fn set_carrier(&self, carrier: bool) {
        *self.inner.carrier.borrow_mut() = carrier;
        self.update_ui();
    }

    pub fn set_device_state(&self, device_state: u32) {
        *self.inner.device_state.borrow_mut() = device_state;
        self.update_ui();
    }

    fn get_signal_icon(&self) -> &'static str {
        let enabled = *self.inner.enabled.borrow();
        let device_state = *self.inner.device_state.borrow();
        let carrier = *self.inner.carrier.borrow();

        if !enabled {
            "network-wired-offline-symbolic"
        } else if device_state == 100 {
            "network-wired-symbolic"
        } else if carrier {
            "network-wired-disconnected-symbolic"
        } else {
            "network-wired-disconnected-symbolic"
        }
    }

    fn update_ui(&self) {
        let enabled = *self.inner.enabled.borrow();
        let device_state = *self.inner.device_state.borrow();
        let carrier = *self.inner.carrier.borrow();

        self.inner.icon.set_icon_name(Some(self.get_signal_icon()));

        let details = if !enabled {
            "Disabled".to_string()
        } else if device_state == 100 {
            "Connected".to_string()
        } else if carrier {
            "Disconnected".to_string()
        } else {
            "No cable".to_string()
        };
        self.inner.details_label.set_label(&details);

        self.inner.switch.set_active(enabled);
    }
}
