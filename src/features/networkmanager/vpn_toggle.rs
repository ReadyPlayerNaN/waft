use gtk::prelude::*;

pub struct VpnToggleWidget {
    pub root: gtk::Box,
}

impl VpnToggleWidget {
    pub fn new() -> Self {
        let root = gtk::Box::new(gtk::Orientation::Horizontal, 0);
        Self { root }
    }

    pub fn widget(&self) -> gtk::Widget {
        self.root.clone().upcast()
    }
}
