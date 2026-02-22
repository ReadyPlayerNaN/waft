use gtk::prelude::*;
use adw::prelude::*;

/// Abstraction over a GTK4 widget container.
///
/// The reconciler calls `vdom_append` to add a new child and `vdom_remove`
/// to detach an old one. The default `vdom_remove` uses `Widget::unparent()`,
/// which works for any parent — including `adw::ActionRow` suffix/prefix slots
/// that have no `remove_suffix` / `remove_prefix` counterpart.
pub trait VdomContainer {
    fn vdom_append(&self, widget: &gtk::Widget);

    fn vdom_remove(&self, widget: &gtk::Widget) {
        widget.unparent();
    }
}

impl VdomContainer for gtk::Box {
    fn vdom_append(&self, widget: &gtk::Widget) {
        self.append(widget);
    }
}

impl VdomContainer for adw::PreferencesGroup {
    fn vdom_append(&self, widget: &gtk::Widget) {
        self.add(widget);
    }
}

/// Wrapper giving a `VdomContainer` impl for the single-child slot of a
/// `gtk::Button`. Uses `set_child()` to place exactly one widget.
pub struct ButtonChildContainer(pub gtk::Button);

impl VdomContainer for ButtonChildContainer {
    fn vdom_append(&self, widget: &gtk::Widget) {
        self.0.set_child(Some(widget));
    }

    fn vdom_remove(&self, _widget: &gtk::Widget) {
        self.0.set_child(gtk::Widget::NONE);
    }
}

/// Wrapper giving a `VdomContainer` impl for the single-child slot of a
/// `gtk::ToggleButton`. Uses `set_child()` to place exactly one widget.
pub struct ToggleButtonChildContainer(pub gtk::ToggleButton);

impl VdomContainer for ToggleButtonChildContainer {
    fn vdom_append(&self, widget: &gtk::Widget) {
        self.0.set_child(Some(widget));
    }

    fn vdom_remove(&self, _widget: &gtk::Widget) {
        self.0.set_child(gtk::Widget::NONE);
    }
}

/// Wrapper giving a `VdomContainer` impl for the **suffix** slot of an
/// `adw::ActionRow`. Uses `add_suffix()` / `Widget::unparent()`.
pub struct ActionRowSuffixContainer(pub adw::ActionRow);

impl VdomContainer for ActionRowSuffixContainer {
    fn vdom_append(&self, widget: &gtk::Widget) {
        self.0.add_suffix(widget);
    }
}

/// Wrapper giving a `VdomContainer` impl for the **prefix** slot of an
/// `adw::ActionRow`. Uses `add_prefix()` / `Widget::unparent()`.
pub struct ActionRowPrefixContainer(pub adw::ActionRow);

impl VdomContainer for ActionRowPrefixContainer {
    fn vdom_append(&self, widget: &gtk::Widget) {
        self.0.add_prefix(widget);
    }
}
