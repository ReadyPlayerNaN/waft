use gtk::prelude::*;
use std::cell::RefCell;
use std::rc::Rc;

#[derive(Clone)]
pub struct WiFiToggleWidget {
    inner: Rc<WiFiToggleWidgetInner>,
}

struct WiFiToggleWidgetInner {
    root: gtk::Box,
    interface_name: String,
    enabled: RefCell<bool>,
    active_ssid: RefCell<Option<String>>,
    network_count: RefCell<usize>,
    icon: gtk::Image,
    details_label: gtk::Label,
    switch: gtk::Switch,
}

impl WiFiToggleWidget {
    pub fn new(interface_name: String) -> Self {
        let root = gtk::Box::builder()
            .orientation(gtk::Orientation::Horizontal)
            .spacing(12)
            .hexpand(true)
            .build();

        let icon = gtk::Image::builder()
            .icon_name("network-wireless-symbolic")
            .pixel_size(24)
            .build();

        let labels_box = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .hexpand(true)
            .build();

        let title_label = gtk::Label::builder()
            .label(&format!("WiFi ({})", interface_name))
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
            inner: Rc::new(WiFiToggleWidgetInner {
                root,
                interface_name,
                enabled: RefCell::new(false),
                active_ssid: RefCell::new(None),
                network_count: RefCell::new(0),
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

    pub fn set_active_ssid(&self, ssid: Option<String>) {
        *self.inner.active_ssid.borrow_mut() = ssid;
        self.update_ui();
    }

    pub fn set_network_count(&self, count: usize) {
        *self.inner.network_count.borrow_mut() = count;
        self.update_ui();
    }

    fn get_signal_icon(&self) -> &'static str {
        if !*self.inner.enabled.borrow() {
            "network-wireless-disabled-symbolic"
        } else if self.inner.active_ssid.borrow().is_some() {
            "network-wireless-symbolic"
        } else {
            "network-wireless-no-route-symbolic"
        }
    }

    fn update_ui(&self) {
        let enabled = *self.inner.enabled.borrow();
        let active_ssid = self.inner.active_ssid.borrow();
        let network_count = *self.inner.network_count.borrow();

        self.inner.icon.set_icon_name(Some(self.get_signal_icon()));

        let details = if !enabled {
            "Disabled".to_string()
        } else if let Some(ref ssid) = *active_ssid {
            ssid.clone()
        } else if network_count > 0 {
            format!("{} networks available", network_count)
        } else {
            "No networks".to_string()
        };
        self.inner.details_label.set_label(&details);

        self.inner.switch.set_active(enabled);
    }
}
