use gtk::prelude::*;

pub struct VpnMenuWidget {
    pub root: gtk::Box,
}

impl VpnMenuWidget {
    pub fn new() -> Self {
        let root = gtk::Box::new(gtk::Orientation::Vertical, 0);
        Self { root }
    }

    pub fn widget(&self) -> gtk::Widget {
        self.root.clone().upcast()
    }
}
