use gtk::prelude::*;
use std::cell::RefCell;

pub struct EthernetMenuWidget {
    pub root: gtk::Box,
    status_label: gtk::Label,
    carrier: RefCell<bool>,
}

impl EthernetMenuWidget {
    pub fn new() -> Self {
        let root = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .spacing(6)
            .margin_top(12)
            .margin_bottom(12)
            .margin_start(12)
            .margin_end(12)
            .build();

        let status_label = gtk::Label::builder()
            .label("Cable unplugged")
            .halign(gtk::Align::Start)
            .build();

        root.append(&status_label);

        Self {
            root,
            status_label,
            carrier: RefCell::new(false),
        }
    }

    pub fn widget(&self) -> gtk::Widget {
        self.root.clone().upcast()
    }

    pub fn set_carrier(&self, carrier: bool) {
        *self.carrier.borrow_mut() = carrier;
        self.status_label.set_label(if carrier {
            "Connected"
        } else {
            "Cable unplugged"
        });
    }
}
