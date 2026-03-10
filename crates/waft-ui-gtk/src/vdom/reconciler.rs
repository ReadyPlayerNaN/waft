use std::any::Any;
use std::rc::Rc;
use std::cell::RefCell;

use gtk::prelude::*;

// Type aliases for complex callback types
type ValueCallback = Rc<RefCell<Option<Rc<dyn Fn(f64)>>>>;
type BoolRefCell = Rc<RefCell<bool>>;
type SourceIdRefCell = Rc<RefCell<Option<glib::SourceId>>>;
use adw::prelude::*;
use gtk::glib;

use super::component::AnyWidget;
use super::container::{ActionRowPrefixContainer, ActionRowSuffixContainer, ButtonChildContainer, ToggleButtonChildContainer, VdomContainer};
use crate::icons::IconWidget;

use super::primitives::{VActionRow, VBox, VButton, VCustomButton, VEntryRow, VIcon, VLabel, VPreferencesGroup, VProgressBar, VRevealer, VScale, VSpinner, VSwitch, VToggleButton, VSwitchRow};
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
    ToggleButton,
    Spinner,
    Icon,
    CustomButton,
    PreferencesGroup,
    ActionRow,
    SwitchRow,
    EntryRow,
    Revealer,
    ProgressBar,
    Scale,
}

// -- Live entries -------------------------------------------------------------

enum ReconcilerEntry {
    Component {
        component:  Box<dyn AnyWidget>,
        last_props: Rc<dyn Any>,
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
        cb:         Option<Rc<dyn Fn()>>,
    },
    Switch {
        widget:     gtk::Switch,
        handler_id: Option<glib::SignalHandlerId>,
        cb:         Option<Rc<dyn Fn(bool)>>,
    },
    ToggleButton {
        widget:           gtk::ToggleButton,
        handler_id:       Option<glib::SignalHandlerId>,
        cb:               Option<Rc<dyn Fn(bool)>>,
        child_reconciler: std::boxed::Box<Reconciler<ToggleButtonChildContainer>>,
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
        cb:               Option<Rc<dyn Fn()>>,
        child_reconciler: std::boxed::Box<Reconciler<ButtonChildContainer>>,
    },
    PreferencesGroup {
        widget:           adw::PreferencesGroup,
        child_reconciler: std::boxed::Box<Reconciler<adw::PreferencesGroup>>,
    },
    ActionRow {
        widget:            adw::ActionRow,
        handler_id:        Option<glib::SignalHandlerId>,
        cb:                Option<Rc<dyn Fn()>>,
        suffix_reconciler: std::boxed::Box<Reconciler<ActionRowSuffixContainer>>,
        prefix_reconciler: std::boxed::Box<Reconciler<ActionRowPrefixContainer>>,
    },
    SwitchRow {
        widget:     adw::SwitchRow,
        handler_id: Option<glib::SignalHandlerId>,
        cb:         Option<Rc<dyn Fn(bool)>>,
    },
    EntryRow {
        widget:     adw::EntryRow,
        handler_id: Option<glib::SignalHandlerId>,
        cb:         Option<Rc<dyn Fn(String)>>,
    },
    Revealer {
        widget:           gtk::Revealer,
        child_reconciler: std::boxed::Box<Reconciler<gtk::Box>>,
    },
    ProgressBar {
        widget: gtk::ProgressBar,
    },
    Scale {
        /// Outer wrapper box that holds the scale and receives gesture controllers.
        scale_wrapper:    gtk::Box,
        widget:           gtk::Scale,
        handler_id:       glib::SignalHandlerId,
        interacting:      Rc<std::cell::RefCell<bool>>,
        #[allow(dead_code)]
        pointer_down:     Rc<std::cell::RefCell<bool>>,
        #[allow(dead_code)]
        debounce_source:  Rc<std::cell::RefCell<Option<glib::SourceId>>>,
        on_value_change:  ValueCallback,
        on_value_commit:  ValueCallback,
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
            Self::ToggleButton     { widget, .. }    => widget.clone().upcast(),
            Self::Spinner          { widget }        => widget.clone().upcast(),
            Self::Icon             { widget }        => widget.widget().clone().upcast(),
            Self::CustomButton     { widget, .. }    => widget.clone().upcast(),
            Self::PreferencesGroup { widget, .. }    => widget.clone().upcast(),
            Self::ActionRow        { widget, .. }    => widget.clone().upcast(),
            Self::SwitchRow        { widget, .. }    => widget.clone().upcast(),
            Self::EntryRow         { widget, .. }    => widget.clone().upcast(),
            Self::Revealer         { widget, .. }    => widget.clone().upcast(),
            Self::ProgressBar      { widget }              => widget.clone().upcast(),
            Self::Scale            { scale_wrapper, .. }   => scale_wrapper.clone().upcast(),
        }
    }

    fn kind_tag(&self) -> KindTag {
        match self {
            Self::Component        { type_id, .. } => KindTag::Component(*type_id),
            Self::Label            { .. }          => KindTag::Label,
            Self::Box              { .. }          => KindTag::Box,
            Self::Button           { .. }          => KindTag::Button,
            Self::Switch           { .. }          => KindTag::Switch,
            Self::ToggleButton     { .. }          => KindTag::ToggleButton,
            Self::Spinner          { .. }          => KindTag::Spinner,
            Self::Icon             { .. }          => KindTag::Icon,
            Self::CustomButton     { .. }          => KindTag::CustomButton,
            Self::PreferencesGroup { .. }          => KindTag::PreferencesGroup,
            Self::ActionRow        { .. }          => KindTag::ActionRow,
            Self::SwitchRow        { .. }          => KindTag::SwitchRow,
            Self::EntryRow         { .. }          => KindTag::EntryRow,
            Self::Revealer         { .. }          => KindTag::Revealer,
            Self::ProgressBar      { .. }          => KindTag::ProgressBar,
            Self::Scale            { .. }          => KindTag::Scale,
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
    key_index: std::collections::HashMap<String, usize>,
    container: C,
}

impl<C: VdomContainer> Reconciler<C> {
    pub fn new(container: C) -> Self {
        Self {
            children:  Vec::new(),
            key_index: std::collections::HashMap::new(),
            container,
        }
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
            if let Some(pos) = self.key_index.remove(key) {
                let (_, entry) = self.children.remove(pos);
                self.container.vdom_remove(&entry.widget());
                // Shift indices above the removed position down by 1.
                for idx in self.key_index.values_mut() {
                    if *idx > pos {
                        *idx -= 1;
                    }
                }
            }
        }

        // 2. Update existing entries and insert new ones.
        // TODO: reorder pre-existing widgets to match new order when required.
        for (key, vnode) in keyed {
            match self.key_index.get(&key).copied() {
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
                    let pos = self.children.len();
                    self.key_index.insert(key.clone(), pos);
                    self.children.push((key, entry));
                }
            }
        }
    }
}

// -- SingleChildReconciler ----------------------------------------------------

/// A reconciler that manages exactly one `VNode` with no container widget.
///
/// Unlike `Reconciler<C>`, which requires a `VdomContainer` to append children
/// into, `SingleChildReconciler` holds the single `ReconcilerEntry` directly
/// and exposes its widget via `widget()`. This is used by `RenderComponent` so
/// that `widget()` returns the rendered widget itself rather than a wrapping
/// `gtk::Box`.
///
/// If the `VNode` kind changes between calls to `reconcile()` the old entry is
/// replaced and the previous widget is orphaned. In practice `RenderFn`
/// implementations always return the same kind so this should never happen;
/// in debug builds an assertion fires to catch violations early.
pub struct SingleChildReconciler {
    entry: Option<ReconcilerEntry>,
}

impl Default for SingleChildReconciler {
    fn default() -> Self {
        Self::new()
    }
}

impl SingleChildReconciler {
    pub fn new() -> Self {
        Self { entry: None }
    }

    pub fn reconcile(&mut self, vnode: VNode) {
        match self.entry.take() {
            None => {
                self.entry = Some(build_entry(vnode));
            }
            Some(mut entry) => {
                if entry.kind_tag() == kind_tag_of(&vnode) {
                    update_entry(&mut entry, vnode);
                    self.entry = Some(entry);
                } else {
                    debug_assert!(
                        false,
                        "SingleChildReconciler: VNode kind changed between renders — \
                         callers holding the old widget() reference are now stale"
                    );
                    self.entry = Some(build_entry(vnode));
                }
            }
        }
    }

    pub fn widget(&self) -> gtk::Widget {
        self.entry
            .as_ref()
            .expect("SingleChildReconciler: widget() called before reconcile()")
            .widget()
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
        VNodeKind::ToggleButton(_)       => KindTag::ToggleButton,
        VNodeKind::Spinner(_)            => KindTag::Spinner,
        VNodeKind::Icon(_)               => KindTag::Icon,
        VNodeKind::CustomButton(_)       => KindTag::CustomButton,
        VNodeKind::PreferencesGroup(_)   => KindTag::PreferencesGroup,
        VNodeKind::ActionRow(_)          => KindTag::ActionRow,
        VNodeKind::SwitchRow(_)          => KindTag::SwitchRow,
        VNodeKind::EntryRow(_)           => KindTag::EntryRow,
        VNodeKind::Revealer(_)           => KindTag::Revealer,
        VNodeKind::ProgressBar(_)        => KindTag::ProgressBar,
        VNodeKind::Scale(_)              => KindTag::Scale,
    }
}

fn build_entry(vnode: VNode) -> ReconcilerEntry {
    match vnode.kind {
        VNodeKind::Component(desc)          => build_component_entry(desc),
        VNodeKind::Label(vlabel)            => build_label_entry(vlabel),
        VNodeKind::Box(vbox)               => build_box_entry(vbox),
        VNodeKind::Button(vbtn)            => build_button_entry(vbtn),
        VNodeKind::Switch(vsw)             => build_switch_entry(vsw),
        VNodeKind::ToggleButton(vtb)       => build_toggle_button_entry(vtb),
        VNodeKind::Spinner(vsp)            => build_spinner_entry(vsp),
        VNodeKind::Icon(vi)                => build_icon_entry(vi),
        VNodeKind::CustomButton(vcb)       => build_custom_button_entry(vcb),
        VNodeKind::PreferencesGroup(vpg)   => build_preferences_group_entry(vpg),
        VNodeKind::ActionRow(vrow)         => build_action_row_entry(vrow),
        VNodeKind::SwitchRow(vsr)          => build_switch_row_entry(vsr),
        VNodeKind::EntryRow(ver)           => build_entry_row_entry(ver),
        VNodeKind::Revealer(vrev)          => build_revealer_entry(vrev),
        VNodeKind::ProgressBar(vpb)        => build_progress_bar_entry(vpb),
        VNodeKind::Scale(vs)               => build_scale_entry(vs),
    }
}

fn build_component_entry(desc: ComponentDesc) -> ReconcilerEntry {
    let component = (desc.build)();
    ReconcilerEntry::Component {
        last_props: Rc::clone(&desc.props),
        type_id:    desc.type_id,
        component,
    }
}

fn build_label_entry(vlabel: VLabel) -> ReconcilerEntry {
    let widget = gtk::Label::new(Some(&vlabel.text));
    apply_label_markup(&widget, &vlabel);
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
    let cb = vbtn.on_click;
    let handler_id = connect_button_handler(&widget, &cb);
    ReconcilerEntry::Button { widget, handler_id, cb }
}

fn build_custom_button_entry(vcb: VCustomButton) -> ReconcilerEntry {
    let widget = gtk::Button::new();
    let classes: Vec<&str> = vcb.css_classes.iter().map(|s| s.as_str()).collect();
    widget.set_css_classes(&classes);
    widget.set_visible(vcb.visible);
    widget.set_sensitive(vcb.sensitive);
    widget.set_hexpand(vcb.hexpand);
    widget.set_vexpand(vcb.vexpand);
    let cb = vcb.on_click;
    let handler_id = connect_button_handler(&widget, &cb);

    let mut child_reconciler: std::boxed::Box<Reconciler<ButtonChildContainer>> =
        std::boxed::Box::new(Reconciler::new(ButtonChildContainer(widget.clone())));
    child_reconciler.reconcile(std::iter::once(*vcb.child));

    ReconcilerEntry::CustomButton { widget, handler_id, cb, child_reconciler }
}

fn build_icon_entry(vi: VIcon) -> ReconcilerEntry {
    let widget = IconWidget::with_fallback(vi.hints, vi.pixel_size, vi.fallback);
    widget.widget().set_visible(vi.visible);
    let classes: Vec<&str> = vi.css_classes.iter().map(|s| s.as_str()).collect();
    widget.widget().set_css_classes(&classes);
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
    let cb = vsw.on_toggle;
    let handler_id = connect_switch_handler(&widget, &cb);
    ReconcilerEntry::Switch { widget, handler_id, cb }
}

fn build_toggle_button_entry(vtb: VToggleButton) -> ReconcilerEntry {
    let widget = gtk::ToggleButton::new();
    widget.set_active(vtb.active);
    widget.set_sensitive(vtb.sensitive);
    let classes: Vec<&str> = vtb.css_classes.iter().map(|s| s.as_str()).collect();
    widget.set_css_classes(&classes);
    let cb = vtb.on_toggle;
    let handler_id = connect_toggle_button_handler(&widget, &cb);

    let mut child_reconciler: std::boxed::Box<Reconciler<ToggleButtonChildContainer>> =
        std::boxed::Box::new(Reconciler::new(ToggleButtonChildContainer(widget.clone())));
    child_reconciler.reconcile(std::iter::once(*vtb.child));

    ReconcilerEntry::ToggleButton { widget, handler_id, cb, child_reconciler }
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

    let cb = vrow.on_activate;
    let handler_id = cb.as_ref().map(|f| {
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

    ReconcilerEntry::ActionRow { widget, handler_id, cb, suffix_reconciler, prefix_reconciler }
}

fn build_switch_row_entry(vsr: VSwitchRow) -> ReconcilerEntry {
    let widget = adw::SwitchRow::new();
    widget.set_title(&vsr.title);
    if let Some(ref s) = vsr.subtitle { widget.set_subtitle(s); }
    widget.set_sensitive(vsr.sensitive);
    // Set active before connecting handler to avoid spurious callback.
    widget.set_active(vsr.active);
    let cb = vsr.on_toggle;
    let handler_id = cb.as_ref().map(|f| {
        let f = f.clone();
        widget.connect_active_notify(move |sw| f(sw.is_active()))
    });
    ReconcilerEntry::SwitchRow { widget, handler_id, cb }
}

fn build_entry_row_entry(ver: VEntryRow) -> ReconcilerEntry {
    let widget = adw::EntryRow::new();
    widget.set_title(&ver.title);
    // Set text before connecting handler to avoid spurious on_change on build.
    widget.set_text(&ver.text);
    widget.set_sensitive(ver.sensitive);
    let cb = ver.on_change;
    let handler_id = cb.as_ref().map(|f| {
        let f = f.clone();
        widget.connect_text_notify(move |er| f(er.text().into()))
    });
    ReconcilerEntry::EntryRow { widget, handler_id, cb }
}

fn build_revealer_entry(vrev: VRevealer) -> ReconcilerEntry {
    let widget = gtk::Revealer::builder()
        .transition_type(vrev.transition_type)
        .transition_duration(vrev.transition_duration)
        .reveal_child(vrev.reveal)
        .build();

    let child_container = gtk::Box::new(gtk::Orientation::Vertical, 0);
    child_container.set_vexpand(vrev.vexpand);
    widget.set_child(Some(&child_container));

    let mut child_reconciler: std::boxed::Box<Reconciler<gtk::Box>> =
        std::boxed::Box::new(Reconciler::new(child_container));
    child_reconciler.reconcile(std::iter::once(*vrev.child));

    ReconcilerEntry::Revealer { widget, child_reconciler }
}

fn build_progress_bar_entry(vpb: VProgressBar) -> ReconcilerEntry {
    let widget = gtk::ProgressBar::new();
    widget.set_fraction(vpb.fraction);
    let classes: Vec<&str> = vpb.css_classes.iter().map(|s| s.as_str()).collect();
    widget.set_css_classes(&classes);
    widget.set_visible(vpb.visible);
    ReconcilerEntry::ProgressBar { widget }
}

/// Cancel any pending debounce for a scale and schedule a new one-shot timer.
///
/// When the timer fires, `interacting` is set to `false` and the commit
/// callback is fired with the user's final value.
fn schedule_scale_interaction_end(
    debounce_source: &SourceIdRefCell,
    interacting: &BoolRefCell,
    scale: &gtk::Scale,
    on_value_commit: &ValueCallback,
    delay_ms: u64,
) {
    if let Some(source_id) = debounce_source.borrow_mut().take() {
        source_id.remove();
    }

    let interacting = interacting.clone();
    let scale = scale.clone();
    let debounce_source_inner = debounce_source.clone();
    let on_value_commit = on_value_commit.clone();

    let source_id = glib::timeout_add_local_once(
        std::time::Duration::from_millis(delay_ms),
        move || {
            *debounce_source_inner.borrow_mut() = None;
            let committed_value = scale.value() / 100.0;
            *interacting.borrow_mut() = false;
            if let Some(ref callback) = *on_value_commit.borrow() {
                callback(committed_value);
            }
        },
    );

    *debounce_source.borrow_mut() = Some(source_id);
}

fn build_scale_entry(vs: VScale) -> ReconcilerEntry {
    let adjustment = gtk::Adjustment::new(vs.value * 100.0, 0.0, 100.0, 1.0, 10.0, 0.0);
    let scale = gtk::Scale::new(gtk::Orientation::Horizontal, Some(&adjustment));
    scale.set_draw_value(false);
    scale.set_hexpand(true);
    let classes: Vec<&str> = vs.css_classes.iter().map(|s| s.as_str()).collect();
    scale.set_css_classes(&classes);

    let scale_wrapper = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    scale_wrapper.set_hexpand(true);
    scale_wrapper.append(&scale);

    let interacting: BoolRefCell = Rc::new(RefCell::new(false));
    let pointer_down: BoolRefCell = Rc::new(RefCell::new(false));
    let debounce_source: SourceIdRefCell = Rc::new(RefCell::new(None));

    // Wrap callbacks in Rc<RefCell<...>> so closures always read the latest version.
    let on_value_change: ValueCallback =
        Rc::new(RefCell::new(vs.on_value_change));
    let on_value_commit: ValueCallback =
        Rc::new(RefCell::new(vs.on_value_commit));

    // Connect value-changed signal
    let on_vc = on_value_change.clone();
    let on_commit_vc = on_value_commit.clone();
    let interacting_vc = interacting.clone();
    let pointer_down_vc = pointer_down.clone();
    let debounce_source_vc = debounce_source.clone();
    let scale_vc = scale.clone();

    let handler_id = scale.connect_value_changed(move |s| {
        let v = s.value() / 100.0;
        if !*pointer_down_vc.borrow() {
            *interacting_vc.borrow_mut() = true;
            schedule_scale_interaction_end(
                &debounce_source_vc,
                &interacting_vc,
                &scale_vc,
                &on_commit_vc,
                200,
            );
        }
        if let Some(ref callback) = *on_vc.borrow() {
            callback(v);
        }
    });

    // GestureClick for press/release detection
    let gesture_click = gtk::GestureClick::new();

    let interacting_pressed = interacting.clone();
    let pointer_down_pressed = pointer_down.clone();
    let debounce_source_pressed = debounce_source.clone();
    gesture_click.connect_pressed(move |_, _, _, _| {
        *pointer_down_pressed.borrow_mut() = true;
        *interacting_pressed.borrow_mut() = true;
        if let Some(source_id) = debounce_source_pressed.borrow_mut().take() {
            source_id.remove();
        }
    });

    let interacting_released = interacting.clone();
    let pointer_down_released = pointer_down.clone();
    let debounce_source_released = debounce_source.clone();
    let scale_released = scale.clone();
    let on_commit_released = on_value_commit.clone();
    gesture_click.connect_released(move |_, _, _, _| {
        *pointer_down_released.borrow_mut() = false;
        schedule_scale_interaction_end(
            &debounce_source_released,
            &interacting_released,
            &scale_released,
            &on_commit_released,
            100,
        );
    });

    let interacting_cancel = interacting.clone();
    let pointer_down_cancel = pointer_down.clone();
    let debounce_source_cancel = debounce_source.clone();
    let scale_cancel = scale.clone();
    let on_commit_cancel = on_value_commit.clone();
    gesture_click.connect_cancel(move |_, _| {
        *pointer_down_cancel.borrow_mut() = false;
        schedule_scale_interaction_end(
            &debounce_source_cancel,
            &interacting_cancel,
            &scale_cancel,
            &on_commit_cancel,
            100,
        );
    });
    scale_wrapper.add_controller(gesture_click);

    // EventControllerScroll for mousewheel
    let scroll_controller =
        gtk::EventControllerScroll::new(gtk::EventControllerScrollFlags::VERTICAL);

    let interacting_scroll = interacting.clone();
    let debounce_source_scroll = debounce_source.clone();
    let scale_scroll = scale.clone();
    let on_commit_scroll = on_value_commit.clone();
    scroll_controller.connect_scroll(move |_, _, _| {
        *interacting_scroll.borrow_mut() = true;
        schedule_scale_interaction_end(
            &debounce_source_scroll,
            &interacting_scroll,
            &scale_scroll,
            &on_commit_scroll,
            200,
        );
        glib::Propagation::Proceed
    });
    scale.add_controller(scroll_controller);

    ReconcilerEntry::Scale {
        scale_wrapper,
        widget: scale,
        handler_id,
        interacting,
        pointer_down,
        debounce_source,
        on_value_change,
        on_value_commit,
    }
}

// -- Update helpers -----------------------------------------------------------

fn update_entry(entry: &mut ReconcilerEntry, vnode: VNode) {
    match (entry, vnode.kind) {
        (ReconcilerEntry::Component { component, last_props, .. }, VNodeKind::Component(desc)) => {
            if !(desc.props_eq)(last_props) {
                (desc.update)(component.as_ref());
                *last_props = Rc::clone(&desc.props);
            }
            // else: props unchanged — no GTK call.
        }
        (ReconcilerEntry::Label { widget }, VNodeKind::Label(vlabel)) => {
            apply_label_markup(widget, &vlabel);
            apply_label_props(widget, &vlabel);
        }
        (ReconcilerEntry::Box { widget, child_reconciler }, VNodeKind::Box(vbox)) => {
            apply_box_props(widget, &vbox);
            child_reconciler.reconcile(vbox.children);
        }
        (ReconcilerEntry::Button { widget, handler_id, cb }, VNodeKind::Button(vbtn)) => {
            widget.set_label(&vbtn.label);
            widget.set_sensitive(vbtn.sensitive);
            if !rc_option_ptr_eq(cb, &vbtn.on_click) {
                if let Some(id) = handler_id.take() { widget.disconnect(id); }
                *handler_id = connect_button_handler(widget, &vbtn.on_click);
                *cb = vbtn.on_click;
            }
        }
        (ReconcilerEntry::Switch { widget, handler_id, cb }, VNodeKind::Switch(vsw)) => {
            let same_cb = rc_option_ptr_eq(cb, &vsw.on_toggle);
            if !same_cb && let Some(id) = handler_id.take() {
                widget.disconnect(id);
            }
            // Set active BEFORE reconnecting handler to avoid spurious callbacks.
            widget.set_active(vsw.active);
            widget.set_sensitive(vsw.sensitive);
            let classes: Vec<&str> = vsw.css_classes.iter().map(|s| s.as_str()).collect();
            widget.set_css_classes(&classes);
            if !same_cb {
                *handler_id = connect_switch_handler(widget, &vsw.on_toggle);
                *cb = vsw.on_toggle;
            }
        }
        (ReconcilerEntry::ToggleButton { widget, handler_id, cb, child_reconciler },
         VNodeKind::ToggleButton(vtb)) => {
            let same_cb = rc_option_ptr_eq(cb, &vtb.on_toggle);
            if !same_cb && let Some(id) = handler_id.take() {
                widget.disconnect(id);
            }
            // Set active BEFORE reconnecting handler to avoid spurious callbacks.
            widget.set_active(vtb.active);
            widget.set_sensitive(vtb.sensitive);
            let classes: Vec<&str> = vtb.css_classes.iter().map(|s| s.as_str()).collect();
            widget.set_css_classes(&classes);
            if !same_cb {
                *handler_id = connect_toggle_button_handler(widget, &vtb.on_toggle);
                *cb = vtb.on_toggle;
            }
            child_reconciler.reconcile(std::iter::once(*vtb.child));
        }
        (ReconcilerEntry::Spinner { widget }, VNodeKind::Spinner(vsp)) => {
            widget.set_spinning(vsp.spinning);
            widget.set_visible(vsp.visible);
        }
        (ReconcilerEntry::Icon { widget }, VNodeKind::Icon(vi)) => {
            widget.update_icon(vi.hints);
            widget.widget().set_visible(vi.visible);
            let classes: Vec<&str> = vi.css_classes.iter().map(|s| s.as_str()).collect();
            widget.widget().set_css_classes(&classes);
        }
        (ReconcilerEntry::CustomButton { widget, handler_id, cb, child_reconciler },
         VNodeKind::CustomButton(vcb)) => {
            let classes: Vec<&str> = vcb.css_classes.iter().map(|s| s.as_str()).collect();
            widget.set_css_classes(&classes);
            widget.set_visible(vcb.visible);
            widget.set_sensitive(vcb.sensitive);
            widget.set_hexpand(vcb.hexpand);
            widget.set_vexpand(vcb.vexpand);
            if !rc_option_ptr_eq(cb, &vcb.on_click) {
                if let Some(id) = handler_id.take() { widget.disconnect(id); }
                *handler_id = connect_button_handler(widget, &vcb.on_click);
                *cb = vcb.on_click;
            }
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
        (ReconcilerEntry::ActionRow { widget, handler_id, cb, suffix_reconciler, prefix_reconciler },
         VNodeKind::ActionRow(vrow)) => {
            widget.set_title(&vrow.title);
            match vrow.subtitle {
                Some(ref s) => widget.set_subtitle(s),
                None        => widget.set_subtitle(""),
            }
            widget.set_activatable(vrow.activatable);
            if !rc_option_ptr_eq(cb, &vrow.on_activate) {
                if let Some(id) = handler_id.take() { widget.disconnect(id); }
                *handler_id = vrow.on_activate.as_ref().map(|f| {
                    let f = f.clone();
                    widget.connect_activated(move |_| f())
                });
                *cb = vrow.on_activate;
            }
            suffix_reconciler.reconcile(vrow.suffix);
            prefix_reconciler.reconcile(vrow.prefix);
        }
        (ReconcilerEntry::SwitchRow { widget, handler_id, cb }, VNodeKind::SwitchRow(vsr)) => {
            widget.set_title(&vsr.title);
            match vsr.subtitle {
                Some(ref s) => widget.set_subtitle(s),
                None        => widget.set_subtitle(""),
            }
            widget.set_sensitive(vsr.sensitive);
            let same_cb = rc_option_ptr_eq(cb, &vsr.on_toggle);
            if !same_cb && let Some(id) = handler_id.take() {
                widget.disconnect(id);
            }
            // Set active AFTER disconnect to suppress spurious callback.
            widget.set_active(vsr.active);
            if !same_cb {
                *handler_id = vsr.on_toggle.as_ref().map(|f| {
                    let f = f.clone();
                    widget.connect_active_notify(move |sw| f(sw.is_active()))
                });
                *cb = vsr.on_toggle;
            }
        }
        (ReconcilerEntry::EntryRow { widget, handler_id, cb }, VNodeKind::EntryRow(ver)) => {
            widget.set_title(&ver.title);
            widget.set_sensitive(ver.sensitive);
            let same_cb = rc_option_ptr_eq(cb, &ver.on_change);
            if !same_cb && let Some(id) = handler_id.take() {
                widget.disconnect(id);
            }
            widget.set_text(&ver.text);
            if !same_cb {
                *handler_id = ver.on_change.as_ref().map(|f| {
                    let f = f.clone();
                    widget.connect_text_notify(move |er| f(er.text().into()))
                });
                *cb = ver.on_change;
            }
        }
        (ReconcilerEntry::ProgressBar { widget }, VNodeKind::ProgressBar(vpb)) => {
            widget.set_fraction(vpb.fraction);
            let classes: Vec<&str> = vpb.css_classes.iter().map(|s| s.as_str()).collect();
            widget.set_css_classes(&classes);
            widget.set_visible(vpb.visible);
        }
        (ReconcilerEntry::Revealer { widget, child_reconciler },
         VNodeKind::Revealer(vrev)) => {
            widget.set_reveal_child(vrev.reveal);
            widget.set_transition_type(vrev.transition_type);
            widget.set_transition_duration(vrev.transition_duration);
            child_reconciler.reconcile(std::iter::once(*vrev.child));
        }
        (ReconcilerEntry::Scale { widget, handler_id, interacting,
                                  on_value_change, on_value_commit, .. },
         VNodeKind::Scale(vs)) => {
            // If the user is actively interacting, skip the backend value update.
            if !*interacting.borrow() {
                widget.block_signal(handler_id);
                widget.set_value(vs.value * 100.0);
                widget.unblock_signal(handler_id);
            }
            let classes: Vec<&str> = vs.css_classes.iter().map(|s| s.as_str()).collect();
            widget.set_css_classes(&classes);
            // Replace the inner callback values. All closures (value-changed
            // handler, gesture handlers, scroll handler) hold Rc-clones of
            // these RefCells and will read the updated value on next invocation.
            *on_value_change.borrow_mut() = vs.on_value_change;
            *on_value_commit.borrow_mut() = vs.on_value_commit;
        }
        // Mismatched arms are prevented by kind_tag_of check above; unreachable.
        _ => unreachable!("update_entry called with mismatched entry and VNodeKind"),
    }
}

// -- Property application helpers ---------------------------------------------

fn apply_label_markup(widget: &gtk::Label, vlabel: &VLabel) {
    match &vlabel.markup {
        Some(m) => {
            widget.set_use_markup(true);
            widget.set_markup(m);
        }
        None => {
            widget.set_use_markup(false);
            widget.set_label(&vlabel.text);
        }
    }
}

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
    widget.set_wrap(vlabel.wrap);
    if let Some(mode) = vlabel.wrap_mode {
        widget.set_wrap_mode(mode);
    }
}

fn apply_box_props(widget: &gtk::Box, vbox: &VBox) {
    let classes: Vec<&str> = vbox.css_classes.iter().map(|s| s.as_str()).collect();
    widget.set_css_classes(&classes);
    if let Some(a) = vbox.valign { widget.set_valign(a); }
    if let Some(a) = vbox.halign { widget.set_halign(a); }
    widget.set_hexpand(vbox.hexpand);
    widget.set_vexpand(vbox.vexpand);
    // orientation and spacing are set at construction and cannot be changed cheaply.
    // If they change, the parent Reconciler rebuilds the entry (kind stays Box,
    // but in practice these fields are always the same for a given slot).
}

/// Returns `true` when both options hold `Rc`s pointing to the same allocation.
fn rc_option_ptr_eq<T: ?Sized>(a: &Option<Rc<T>>, b: &Option<Rc<T>>) -> bool {
    match (a, b) {
        (Some(a), Some(b)) => Rc::ptr_eq(a, b),
        (None,    None)    => true,
        _                  => false,
    }
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

fn connect_toggle_button_handler(
    widget: &gtk::ToggleButton,
    on_toggle: &Option<std::rc::Rc<dyn Fn(bool)>>,
) -> Option<glib::SignalHandlerId> {
    on_toggle.as_ref().map(|f| {
        let f = f.clone();
        widget.connect_toggled(move |tb| f(tb.is_active()))
    })
}
