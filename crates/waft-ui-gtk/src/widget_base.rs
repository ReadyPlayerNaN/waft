//! Common trait for all waft widgets providing access to the underlying GTK widget.

/// Common trait for all waft widgets providing access to the underlying GTK widget.
pub trait WidgetBase {
    fn widget(&self) -> gtk::Widget;
}

/// A single child widget.
pub enum Child {
    Base(Box<dyn WidgetBase>),
    Gtk(gtk::Widget),
}

impl Child {
    pub fn widget(&self) -> gtk::Widget {
        match self {
            Child::Base(b) => b.widget(),
            Child::Gtk(w) => w.clone(),
        }
    }
}

impl From<gtk::Widget> for Child {
    fn from(w: gtk::Widget) -> Self {
        Child::Gtk(w)
    }
}

/// Children for a container widget — one or many.
pub enum Children {
    One(Child),
    Many(Vec<Child>),
}

impl Children {
    /// Iterate over all children as GTK widgets.
    pub fn iter_widgets(&self) -> impl Iterator<Item = gtk::Widget> + '_ {
        let items: Box<dyn Iterator<Item = gtk::Widget> + '_> = match self {
            Children::One(child) => Box::new(std::iter::once(child.widget())),
            Children::Many(children) => Box::new(children.iter().map(Child::widget)),
        };
        items
    }
}

impl From<Vec<Child>> for Children {
    fn from(v: Vec<Child>) -> Self {
        Children::Many(v)
    }
}

impl From<Child> for Children {
    fn from(c: Child) -> Self {
        Children::One(c)
    }
}

impl From<gtk::Widget> for Children {
    fn from(w: gtk::Widget) -> Self {
        Children::One(Child::Gtk(w))
    }
}
