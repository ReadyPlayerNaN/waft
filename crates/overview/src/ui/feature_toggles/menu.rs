use glib::object::{Cast, IsA};
use gtk::prelude::{BoxExt, WidgetExt};

#[derive(Clone)]
pub struct FeatureToggleMenuWidget {
    pub root: gtk::Box,
}

impl FeatureToggleMenuWidget {
    pub fn new() -> Self {
        let root = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .spacing(0)
            .css_classes(["menu-content"])
            .build();
        Self { root }
    }

    pub fn append(&self, child: &impl IsA<gtk::Widget>) {
        self.root.append(child);
    }

    pub fn insert_child_after(
        &self,
        child: &impl IsA<gtk::Widget>,
        sibling: Option<&impl IsA<gtk::Widget>>,
    ) {
        self.root.insert_child_after(child, sibling);
    }

    pub fn last_child(&self) -> Option<gtk::Widget> {
        self.root.last_child()
    }

    pub fn reorder_child_after(
        &self,
        child: &impl IsA<gtk::Widget>,
        sibling: Option<&impl IsA<gtk::Widget>>,
    ) {
        self.root.reorder_child_after(child, sibling);
    }

    pub fn remove(&self, child: &impl IsA<gtk::Widget>) {
        self.root.remove(child);
    }

    pub fn root(&self) -> &gtk::Box {
        &self.root
    }

    pub fn widget(&self) -> &gtk::Widget {
        self.root.upcast_ref::<gtk::Widget>()
    }
}
