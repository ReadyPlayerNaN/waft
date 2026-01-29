use gtk::prelude::*;
use std::cell::RefCell;
use std::rc::Rc;

pub struct EthernetToggleWidget {
    pub root: gtk::Box,
    interface_name: String,
    enabled: Rc<RefCell<bool>>,
    carrier: Rc<RefCell<bool>>,
    active_connection: Rc<RefCell<Option<String>>>,
    icon: gtk::Image,
    details_label: gtk::Label,
    switch: gtk::Switch,
}

impl EthernetToggleWidget {
    pub fn new(interface_name: String) -> Self {
        let root = gtk::Box::builder()
            .orientation(gtk::Orientation::Horizontal)
            .spacing(12)
            .hexpand(true)
            .build();

        let icon = gtk::Image::builder()
            .icon_name("network-wired-disconnected-symbolic")
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
            root,
            interface_name,
            enabled: Rc::new(RefCell::new(false)),
            carrier: Rc::new(RefCell::new(false)),
            active_connection: Rc::new(RefCell::new(None)),
            icon,
            details_label,
            switch,
        }
    }

    pub fn widget(&self) -> gtk::Widget {
        self.root.clone().upcast()
    }

    pub fn set_enabled(&self, enabled: bool) {
        *self.enabled.borrow_mut() = enabled;
        self.update_ui();
    }

    pub fn set_carrier(&self, carrier: bool) {
        *self.carrier.borrow_mut() = carrier;
        self.update_ui();
    }

    pub fn set_active_connection(&self, connection: Option<String>) {
        *self.active_connection.borrow_mut() = connection;
        self.update_ui();
    }

    fn update_ui(&self) {
        let enabled = *self.enabled.borrow();
        let carrier = *self.carrier.borrow();
        let active_connection = self.active_connection.borrow();

        let icon_name = if enabled && carrier {
            "network-wired-symbolic"
        } else {
            "network-wired-disconnected-symbolic"
        };
        self.icon.set_icon_name(Some(icon_name));

        let details = if !enabled {
            "Disabled".to_string()
        } else if !carrier {
            "Cable unplugged".to_string()
        } else if let Some(ref conn) = *active_connection {
            conn.clone()
        } else {
            "Available".to_string()
        };
        self.details_label.set_label(&details);

        self.switch.set_active(enabled && carrier);
    }
}
