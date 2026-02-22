use std::any::Any;

use gtk::glib;
use gtk::prelude::*;

use super::component::AnyWidget;
use crate::icons::IconWidget;

use super::primitives::{VBox, VButton, VIcon, VLabel, VSpinner, VSwitch};
use super::vnode::{ComponentDesc, VNode, VNodeKind};

// -- Kind tag -----------------------------------------------------------------
// Used to detect mismatches that require destroy-and-rebuild.

#[derive(PartialEq)]
enum KindTag {
    Component(std::any::TypeId),
    Label,
    Box,
    Button,
    Switch,
    Spinner,
    Icon,
}

// -- Live entries -------------------------------------------------------------

enum ReconcilerEntry {
    Component {
        component:  Box<dyn AnyWidget>,
        last_props: Box<dyn Any>,
        type_id:    std::any::TypeId,
    },
    Label {
        widget: gtk::Label,
    },
    Box {
        widget:           gtk::Box,
        child_reconciler: std::boxed::Box<Reconciler>,
    },
    Button {
        widget:     gtk::Button,
        handler_id: Option<glib::SignalHandlerId>,
    },
    Switch {
        widget:     gtk::Switch,
        handler_id: Option<glib::SignalHandlerId>,
    },
    Spinner {
        widget: gtk::Spinner,
    },
    Icon {
        widget: IconWidget,
    },
}

impl ReconcilerEntry {
    fn widget(&self) -> gtk::Widget {
        match self {
            Self::Component { component, .. } => component.widget(),
            Self::Label   { widget }          => widget.clone().upcast(),
            Self::Box     { widget, .. }      => widget.clone().upcast(),
            Self::Button  { widget, .. }      => widget.clone().upcast(),
            Self::Switch  { widget, .. }      => widget.clone().upcast(),
            Self::Spinner { widget }          => widget.clone().upcast(),
            Self::Icon    { widget }          => widget.widget().clone().upcast(),
        }
    }

    fn kind_tag(&self) -> KindTag {
        match self {
            Self::Component { type_id, .. } => KindTag::Component(*type_id),
            Self::Label   { .. }            => KindTag::Label,
            Self::Box     { .. }            => KindTag::Box,
            Self::Button  { .. }            => KindTag::Button,
            Self::Switch  { .. }            => KindTag::Switch,
            Self::Spinner { .. }            => KindTag::Spinner,
            Self::Icon    { .. }            => KindTag::Icon,
        }
    }
}

// -- Reconciler ---------------------------------------------------------------

/// Maintains a keyed list of live component or primitive instances inside a
/// `gtk::Box`. Call `reconcile()` with a new list of `VNode`s on every state
/// change.
///
/// Operations per call:
/// - **Key present, same kind, props unchanged** → kept as-is (components only).
/// - **Key present, same kind, props changed** → widget updated in place.
/// - **Key present, kind changed** → old widget removed, new one built.
/// - **Key absent from new list** → widget removed from container.
pub struct Reconciler {
    children:  Vec<(String, ReconcilerEntry)>,
    container: gtk::Box,
}

impl Reconciler {
    pub fn new(container: gtk::Box) -> Self {
        Self { children: Vec::new(), container }
    }

    pub fn reconcile(&mut self, nodes: impl IntoIterator<Item = VNode>) {
        let nodes: Vec<VNode> = nodes.into_iter().collect();

        // Assign keys: explicit key or positional fallback "$0", "$1", …
        let keyed: Vec<(String, VNode)> = nodes
            .into_iter()
            .enumerate()
            .map(|(i, node)| {
                let key = node.key.clone().unwrap_or_else(|| format!("${i}"));
                (key, node)
            })
            .collect();

        let new_keys: std::collections::HashSet<&str> =
            keyed.iter().map(|(k, _)| k.as_str()).collect();

        // 1. Remove entries absent from the new list.
        let to_remove: Vec<String> = self
            .children
            .iter()
            .filter(|(k, _)| !new_keys.contains(k.as_str()))
            .map(|(k, _)| k.clone())
            .collect();

        for key in &to_remove {
            let pos = self
                .children
                .iter()
                .position(|(k, _)| k == key)
                .expect("key in to_remove must exist in children");
            let (_, entry) = self.children.remove(pos);
            self.container.remove(&entry.widget());
        }

        // 2. Update existing entries and insert new ones.
        // TODO: reorder pre-existing widgets to match new order when required.
        for (key, vnode) in keyed {
            match self.children.iter().position(|(k, _)| k == &key) {
                Some(pos) => {
                    let new_tag = kind_tag_of(&vnode);
                    let old_tag = self.children[pos].1.kind_tag();

                    if old_tag != new_tag {
                        // Kind changed: destroy old widget, build new one.
                        self.container.remove(&self.children[pos].1.widget());
                        let entry = build_entry(vnode);
                        self.container.append(&entry.widget());
                        self.children[pos].1 = entry;
                    } else {
                        // Same kind: update in place.
                        update_entry(&mut self.children[pos].1, vnode);
                    }
                }

                None => {
                    // New key: build and append.
                    let entry = build_entry(vnode);
                    self.container.append(&entry.widget());
                    self.children.push((key, entry));
                }
            }
        }
    }
}

// -- Build helpers ------------------------------------------------------------

fn kind_tag_of(vnode: &VNode) -> KindTag {
    match &vnode.kind {
        VNodeKind::Component(desc) => KindTag::Component(desc.type_id),
        VNodeKind::Label(_)        => KindTag::Label,
        VNodeKind::Box(_)          => KindTag::Box,
        VNodeKind::Button(_)       => KindTag::Button,
        VNodeKind::Switch(_)       => KindTag::Switch,
        VNodeKind::Spinner(_)      => KindTag::Spinner,
        VNodeKind::Icon(_)         => KindTag::Icon,
    }
}

fn build_entry(vnode: VNode) -> ReconcilerEntry {
    match vnode.kind {
        VNodeKind::Component(desc)  => build_component_entry(desc),
        VNodeKind::Label(vlabel)    => build_label_entry(vlabel),
        VNodeKind::Box(vbox)        => build_box_entry(vbox),
        VNodeKind::Button(vbtn)     => build_button_entry(vbtn),
        VNodeKind::Switch(vsw)      => build_switch_entry(vsw),
        VNodeKind::Spinner(vsp)     => build_spinner_entry(vsp),
        VNodeKind::Icon(vi)         => build_icon_entry(vi),
    }
}

fn build_component_entry(desc: ComponentDesc) -> ReconcilerEntry {
    let component = (desc.build)();
    ReconcilerEntry::Component {
        last_props: desc.props,
        type_id:    desc.type_id,
        component,
    }
}

fn build_label_entry(vlabel: VLabel) -> ReconcilerEntry {
    let widget = gtk::Label::new(Some(&vlabel.text));
    apply_label_props(&widget, &vlabel);
    ReconcilerEntry::Label { widget }
}

fn build_box_entry(vbox: VBox) -> ReconcilerEntry {
    let widget = gtk::Box::new(vbox.orientation, vbox.spacing);
    apply_box_props(&widget, &vbox);
    let mut child_reconciler = std::boxed::Box::new(Reconciler::new(widget.clone()));
    child_reconciler.reconcile(vbox.children);
    ReconcilerEntry::Box { widget, child_reconciler }
}

fn build_button_entry(vbtn: VButton) -> ReconcilerEntry {
    let widget = gtk::Button::with_label(&vbtn.label);
    widget.set_sensitive(vbtn.sensitive);
    let handler_id = connect_button_handler(&widget, &vbtn.on_click);
    ReconcilerEntry::Button { widget, handler_id }
}

fn build_icon_entry(vi: VIcon) -> ReconcilerEntry {
    let widget = IconWidget::new(vi.hints, vi.pixel_size);
    widget.widget().set_visible(vi.visible);
    ReconcilerEntry::Icon { widget }
}

fn build_spinner_entry(vsp: VSpinner) -> ReconcilerEntry {
    let widget = gtk::Spinner::new();
    widget.set_spinning(vsp.spinning);
    widget.set_visible(vsp.visible);
    ReconcilerEntry::Spinner { widget }
}

fn build_switch_entry(vsw: VSwitch) -> ReconcilerEntry {
    let widget = gtk::Switch::new();
    widget.set_active(vsw.active);
    widget.set_sensitive(vsw.sensitive);
    let classes: Vec<&str> = vsw.css_classes.iter().map(|s| s.as_str()).collect();
    widget.set_css_classes(&classes);
    let handler_id = connect_switch_handler(&widget, &vsw.on_toggle);
    ReconcilerEntry::Switch { widget, handler_id }
}

// -- Update helpers -----------------------------------------------------------

fn update_entry(entry: &mut ReconcilerEntry, vnode: VNode) {
    match (entry, vnode.kind) {
        (ReconcilerEntry::Component { component, last_props, .. }, VNodeKind::Component(desc)) => {
            if !(desc.props_eq)(last_props) {
                (desc.update)(component.as_ref());
                *last_props = desc.props;
            }
            // else: props unchanged — no GTK call.
        }
        (ReconcilerEntry::Label { widget }, VNodeKind::Label(vlabel)) => {
            widget.set_label(&vlabel.text);
            apply_label_props(widget, &vlabel);
        }
        (ReconcilerEntry::Box { widget, child_reconciler }, VNodeKind::Box(vbox)) => {
            apply_box_props(widget, &vbox);
            child_reconciler.reconcile(vbox.children);
        }
        (ReconcilerEntry::Button { widget, handler_id }, VNodeKind::Button(vbtn)) => {
            widget.set_label(&vbtn.label);
            widget.set_sensitive(vbtn.sensitive);
            // Callbacks have no identity — always disconnect old, connect new.
            if let Some(id) = handler_id.take() {
                widget.disconnect(id);
            }
            *handler_id = connect_button_handler(widget, &vbtn.on_click);
        }
        (ReconcilerEntry::Switch { widget, handler_id }, VNodeKind::Switch(vsw)) => {
            if let Some(id) = handler_id.take() {
                widget.disconnect(id);
            }
            // Set active BEFORE reconnecting handler to avoid spurious callbacks.
            widget.set_active(vsw.active);
            widget.set_sensitive(vsw.sensitive);
            let classes: Vec<&str> = vsw.css_classes.iter().map(|s| s.as_str()).collect();
            widget.set_css_classes(&classes);
            *handler_id = connect_switch_handler(widget, &vsw.on_toggle);
        }
        (ReconcilerEntry::Spinner { widget }, VNodeKind::Spinner(vsp)) => {
            widget.set_spinning(vsp.spinning);
            widget.set_visible(vsp.visible);
        }
        (ReconcilerEntry::Icon { widget }, VNodeKind::Icon(vi)) => {
            widget.update_icon(vi.hints);
            widget.widget().set_visible(vi.visible);
        }
        // Mismatched arms are prevented by kind_tag_of check above; unreachable.
        _ => unreachable!("update_entry called with mismatched entry and VNodeKind"),
    }
}

// -- Property application helpers ---------------------------------------------

fn apply_label_props(widget: &gtk::Label, vlabel: &VLabel) {
    let classes: Vec<&str> = vlabel.css_classes.iter().map(|s| s.as_str()).collect();
    widget.set_css_classes(&classes);
    if let Some(x) = vlabel.xalign {
        widget.set_xalign(x);
    }
    widget.set_hexpand(vlabel.hexpand);
    if let Some(mode) = vlabel.ellipsize {
        widget.set_ellipsize(mode);
    }
}

fn apply_box_props(widget: &gtk::Box, vbox: &VBox) {
    let classes: Vec<&str> = vbox.css_classes.iter().map(|s| s.as_str()).collect();
    widget.set_css_classes(&classes);
    if let Some(a) = vbox.valign { widget.set_valign(a); }
    if let Some(a) = vbox.halign { widget.set_halign(a); }
    // orientation and spacing are set at construction and cannot be changed cheaply.
    // If they change, the parent Reconciler rebuilds the entry (kind stays Box,
    // but in practice these fields are always the same for a given slot).
}

fn connect_button_handler(
    widget: &gtk::Button,
    on_click: &Option<std::rc::Rc<dyn Fn()>>,
) -> Option<glib::SignalHandlerId> {
    on_click.as_ref().map(|f| {
        let f = f.clone();
        widget.connect_clicked(move |_| f())
    })
}

fn connect_switch_handler(
    widget: &gtk::Switch,
    on_toggle: &Option<std::rc::Rc<dyn Fn(bool)>>,
) -> Option<glib::SignalHandlerId> {
    on_toggle.as_ref().map(|f| {
        let f = f.clone();
        widget.connect_active_notify(move |sw| f(sw.is_active()))
    })
}
