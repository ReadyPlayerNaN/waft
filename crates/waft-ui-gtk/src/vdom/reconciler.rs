use std::any::Any;

use gtk::prelude::*;
use adw::prelude::*;
use gtk::glib;

use super::component::AnyWidget;
use super::container::{ActionRowPrefixContainer, ActionRowSuffixContainer, VdomContainer};
use crate::icons::IconWidget;

use super::primitives::{VActionRow, VBox, VButton, VCustomButton, VIcon, VLabel, VPreferencesGroup, VSpinner, VSwitch};
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
    CustomButton,
    PreferencesGroup,
    ActionRow,
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
        child_reconciler: std::boxed::Box<Reconciler<gtk::Box>>,
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
    CustomButton {
        widget:           gtk::Button,
        handler_id:       Option<glib::SignalHandlerId>,
        child_reconciler: std::boxed::Box<Reconciler<gtk::Box>>,
    },
    PreferencesGroup {
        widget:           adw::PreferencesGroup,
        child_reconciler: std::boxed::Box<Reconciler<adw::PreferencesGroup>>,
    },
    ActionRow {
        widget:            adw::ActionRow,
        handler_id:        Option<glib::SignalHandlerId>,
        suffix_reconciler: std::boxed::Box<Reconciler<ActionRowSuffixContainer>>,
        prefix_reconciler: std::boxed::Box<Reconciler<ActionRowPrefixContainer>>,
    },
}

impl ReconcilerEntry {
    fn widget(&self) -> gtk::Widget {
        match self {
            Self::Component        { component, .. } => component.widget(),
            Self::Label            { widget }        => widget.clone().upcast(),
            Self::Box              { widget, .. }    => widget.clone().upcast(),
            Self::Button           { widget, .. }    => widget.clone().upcast(),
            Self::Switch           { widget, .. }    => widget.clone().upcast(),
            Self::Spinner          { widget }        => widget.clone().upcast(),
            Self::Icon             { widget }        => widget.widget().clone().upcast(),
            Self::CustomButton     { widget, .. }    => widget.clone().upcast(),
            Self::PreferencesGroup { widget, .. }    => widget.clone().upcast(),
            Self::ActionRow        { widget, .. }    => widget.clone().upcast(),
        }
    }

    fn kind_tag(&self) -> KindTag {
        match self {
            Self::Component        { type_id, .. } => KindTag::Component(*type_id),
            Self::Label            { .. }          => KindTag::Label,
            Self::Box              { .. }          => KindTag::Box,
            Self::Button           { .. }          => KindTag::Button,
            Self::Switch           { .. }          => KindTag::Switch,
            Self::Spinner          { .. }          => KindTag::Spinner,
            Self::Icon             { .. }          => KindTag::Icon,
            Self::CustomButton     { .. }          => KindTag::CustomButton,
            Self::PreferencesGroup { .. }          => KindTag::PreferencesGroup,
            Self::ActionRow        { .. }          => KindTag::ActionRow,
        }
    }
}

// -- Reconciler ---------------------------------------------------------------

/// Maintains a keyed list of live component or primitive instances inside a
/// container widget. Call `reconcile()` with a new list of `VNode`s on every
/// state change.
///
/// The type parameter `C` must implement `VdomContainer`. It defaults to
/// `gtk::Box`, so all existing call sites compile unchanged.
///
/// Operations per call:
/// - **Key present, same kind, props unchanged** → kept as-is (components only).
/// - **Key present, same kind, props changed** → widget updated in place.
/// - **Key present, kind changed** → old widget removed, new one built.
/// - **Key absent from new list** → widget removed from container.
pub struct Reconciler<C: VdomContainer = gtk::Box> {
    children:  Vec<(String, ReconcilerEntry)>,
    container: C,
}

impl<C: VdomContainer> Reconciler<C> {
    pub fn new(container: C) -> Self {
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
            self.container.vdom_remove(&entry.widget());
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
                        self.container.vdom_remove(&self.children[pos].1.widget());
                        let entry = build_entry(vnode);
                        self.container.vdom_append(&entry.widget());
                        self.children[pos].1 = entry;
                    } else {
                        // Same kind: update in place.
                        update_entry(&mut self.children[pos].1, vnode);
                    }
                }

                None => {
                    // New key: build and append.
                    let entry = build_entry(vnode);
                    self.container.vdom_append(&entry.widget());
                    self.children.push((key, entry));
                }
            }
        }
    }
}

// -- Build helpers ------------------------------------------------------------

fn kind_tag_of(vnode: &VNode) -> KindTag {
    match &vnode.kind {
        VNodeKind::Component(desc)       => KindTag::Component(desc.type_id),
        VNodeKind::Label(_)              => KindTag::Label,
        VNodeKind::Box(_)                => KindTag::Box,
        VNodeKind::Button(_)             => KindTag::Button,
        VNodeKind::Switch(_)             => KindTag::Switch,
        VNodeKind::Spinner(_)            => KindTag::Spinner,
        VNodeKind::Icon(_)               => KindTag::Icon,
        VNodeKind::CustomButton(_)       => KindTag::CustomButton,
        VNodeKind::PreferencesGroup(_)   => KindTag::PreferencesGroup,
        VNodeKind::ActionRow(_)          => KindTag::ActionRow,
    }
}

fn build_entry(vnode: VNode) -> ReconcilerEntry {
    match vnode.kind {
        VNodeKind::Component(desc)          => build_component_entry(desc),
        VNodeKind::Label(vlabel)            => build_label_entry(vlabel),
        VNodeKind::Box(vbox)               => build_box_entry(vbox),
        VNodeKind::Button(vbtn)            => build_button_entry(vbtn),
        VNodeKind::Switch(vsw)             => build_switch_entry(vsw),
        VNodeKind::Spinner(vsp)            => build_spinner_entry(vsp),
        VNodeKind::Icon(vi)                => build_icon_entry(vi),
        VNodeKind::CustomButton(vcb)       => build_custom_button_entry(vcb),
        VNodeKind::PreferencesGroup(vpg)   => build_preferences_group_entry(vpg),
        VNodeKind::ActionRow(vrow)         => build_action_row_entry(vrow),
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
    let mut child_reconciler: std::boxed::Box<Reconciler<gtk::Box>> =
        std::boxed::Box::new(Reconciler::new(widget.clone()));
    child_reconciler.reconcile(vbox.children);
    ReconcilerEntry::Box { widget, child_reconciler }
}

fn build_button_entry(vbtn: VButton) -> ReconcilerEntry {
    let widget = gtk::Button::with_label(&vbtn.label);
    widget.set_sensitive(vbtn.sensitive);
    let handler_id = connect_button_handler(&widget, &vbtn.on_click);
    ReconcilerEntry::Button { widget, handler_id }
}

fn build_custom_button_entry(vcb: VCustomButton) -> ReconcilerEntry {
    let child_container = gtk::Box::new(gtk::Orientation::Vertical, 0);
    let mut child_reconciler: std::boxed::Box<Reconciler<gtk::Box>> =
        std::boxed::Box::new(Reconciler::new(child_container.clone()));
    child_reconciler.reconcile(std::iter::once(*vcb.child));

    let widget = gtk::Button::new();
    widget.set_child(Some(&child_container));
    let classes: Vec<&str> = vcb.css_classes.iter().map(|s| s.as_str()).collect();
    widget.set_css_classes(&classes);
    widget.set_visible(vcb.visible);
    widget.set_sensitive(vcb.sensitive);
    let handler_id = connect_button_handler(&widget, &vcb.on_click);

    ReconcilerEntry::CustomButton { widget, handler_id, child_reconciler }
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

fn build_preferences_group_entry(vpg: VPreferencesGroup) -> ReconcilerEntry {
    let widget = adw::PreferencesGroup::new();
    if let Some(ref t) = vpg.title { widget.set_title(t); }
    let mut child_reconciler: std::boxed::Box<Reconciler<adw::PreferencesGroup>> =
        std::boxed::Box::new(Reconciler::new(widget.clone()));
    child_reconciler.reconcile(vpg.children);
    ReconcilerEntry::PreferencesGroup { widget, child_reconciler }
}

fn build_action_row_entry(vrow: VActionRow) -> ReconcilerEntry {
    let widget = adw::ActionRow::new();
    widget.set_title(&vrow.title);
    if let Some(ref s) = vrow.subtitle { widget.set_subtitle(s); }
    widget.set_activatable(vrow.activatable);

    let handler_id = vrow.on_activate.as_ref().map(|f| {
        let f = f.clone();
        widget.connect_activated(move |_| f())
    });

    let mut suffix_reconciler = std::boxed::Box::new(
        Reconciler::new(ActionRowSuffixContainer(widget.clone()))
    );
    suffix_reconciler.reconcile(vrow.suffix);

    let mut prefix_reconciler = std::boxed::Box::new(
        Reconciler::new(ActionRowPrefixContainer(widget.clone()))
    );
    prefix_reconciler.reconcile(vrow.prefix);

    ReconcilerEntry::ActionRow { widget, handler_id, suffix_reconciler, prefix_reconciler }
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
        (ReconcilerEntry::CustomButton { widget, handler_id, child_reconciler },
         VNodeKind::CustomButton(vcb)) => {
            let classes: Vec<&str> = vcb.css_classes.iter().map(|s| s.as_str()).collect();
            widget.set_css_classes(&classes);
            widget.set_visible(vcb.visible);
            widget.set_sensitive(vcb.sensitive);
            if let Some(id) = handler_id.take() { widget.disconnect(id); }
            *handler_id = connect_button_handler(widget, &vcb.on_click);
            child_reconciler.reconcile(std::iter::once(*vcb.child));
        }
        (ReconcilerEntry::PreferencesGroup { widget, child_reconciler },
         VNodeKind::PreferencesGroup(vpg)) => {
            match vpg.title {
                Some(ref t) => widget.set_title(t),
                None        => widget.set_title(""),
            }
            child_reconciler.reconcile(vpg.children);
        }
        (ReconcilerEntry::ActionRow { widget, handler_id, suffix_reconciler, prefix_reconciler },
         VNodeKind::ActionRow(vrow)) => {
            widget.set_title(&vrow.title);
            match vrow.subtitle {
                Some(ref s) => widget.set_subtitle(s),
                None        => widget.set_subtitle(""),
            }
            widget.set_activatable(vrow.activatable);
            if let Some(id) = handler_id.take() { widget.disconnect(id); }
            *handler_id = vrow.on_activate.as_ref().map(|f| {
                let f = f.clone();
                widget.connect_activated(move |_| f())
            });
            suffix_reconciler.reconcile(vrow.suffix);
            prefix_reconciler.reconcile(vrow.prefix);
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
