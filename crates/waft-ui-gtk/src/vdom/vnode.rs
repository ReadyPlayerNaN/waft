use std::any::{Any, TypeId};
use std::rc::Rc;

use super::component::{AnyWidget, Component};
use super::primitives::{VActionRow, VBox, VButton, VCustomButton, VEntryRow, VIcon, VLabel, VPreferencesGroup, VProgressBar, VRevealer, VScale, VSpinner, VSwitch, VToggleButton, VSwitchRow};

// -- Component descriptor (what VNode currently stores at the top level) ---

type BuildFn   = Rc<dyn Fn() -> Box<dyn AnyWidget>>;
type UpdateFn  = Rc<dyn Fn(&dyn AnyWidget)>;
type PropsEqFn = Rc<dyn Fn(&Rc<dyn Any>) -> bool>;

pub(super) struct ComponentDesc {
    pub(super) type_id:  TypeId,
    pub(super) build:    BuildFn,
    pub(super) update:   UpdateFn,
    pub(super) props_eq: PropsEqFn,
    pub(super) props:    Rc<dyn Any>,
}

// -- VNodeKind ----------------------------------------------------------------

pub(super) enum VNodeKind {
    Component(ComponentDesc),
    Label(VLabel),
    Box(VBox),
    Button(VButton),
    Switch(VSwitch),
    ToggleButton(VToggleButton),
    Spinner(VSpinner),
    Icon(VIcon),
    CustomButton(VCustomButton),
    PreferencesGroup(VPreferencesGroup),
    ActionRow(VActionRow),
    SwitchRow(VSwitchRow),
    EntryRow(VEntryRow),
    Revealer(VRevealer),
    ProgressBar(VProgressBar),
    Scale(VScale),
}

// -- VNode --------------------------------------------------------------------

/// A type-erased description of a UI element — either a custom `Component`
/// or a GTK primitive — with optional reconciliation key and output handler.
pub struct VNode {
    pub(super) key:  Option<String>,
    pub(super) kind: VNodeKind,
}

impl VNode {
    // -- Component constructors (existing public API — unchanged) ----------

    /// Component with no output events.
    pub fn new<C: Component>(props: C::Props) -> Self {
        Self { key: None, kind: VNodeKind::Component(make_component_desc::<C>(props, |_| {})) }
    }

    /// Component with an output handler.
    pub fn with_output<C: Component>(
        props: C::Props,
        on_output: impl Fn(C::Output) + 'static,
    ) -> Self {
        Self { key: None, kind: VNodeKind::Component(make_component_desc::<C>(props, on_output)) }
    }

    /// Set the reconciliation key. Use a stable identifier (e.g. URN string).
    pub fn key(mut self, key: impl Into<String>) -> Self {
        self.key = Some(key.into());
        self
    }

    // -- Primitive constructors -------------------------------------------

    /// Build a `VLabel` descriptor and wrap it in a `VNode`.
    pub fn label(v: VLabel) -> Self {
        Self { key: None, kind: VNodeKind::Label(v) }
    }

    /// Build a `VBox` descriptor and wrap it in a `VNode`.
    pub fn vbox(v: VBox) -> Self {
        Self { key: None, kind: VNodeKind::Box(v) }
    }

    /// Build a `VButton` descriptor and wrap it in a `VNode`.
    pub fn button(v: VButton) -> Self {
        Self { key: None, kind: VNodeKind::Button(v) }
    }

    /// Build a `VSwitch` descriptor and wrap it in a `VNode`.
    pub fn switch(v: VSwitch) -> Self {
        Self { key: None, kind: VNodeKind::Switch(v) }
    }

    /// Build a `VToggleButton` descriptor and wrap it in a `VNode`.
    pub fn toggle_button(v: VToggleButton) -> Self {
        Self { key: None, kind: VNodeKind::ToggleButton(v) }
    }

    /// Build a `VSpinner` descriptor and wrap it in a `VNode`.
    pub fn spinner(v: VSpinner) -> Self {
        Self { key: None, kind: VNodeKind::Spinner(v) }
    }

    /// Build a `VIcon` descriptor and wrap it in a `VNode`.
    pub fn icon(v: VIcon) -> Self {
        Self { key: None, kind: VNodeKind::Icon(v) }
    }

    /// Build a `VCustomButton` descriptor and wrap it in a `VNode`.
    pub fn custom_button(v: VCustomButton) -> Self {
        Self { key: None, kind: VNodeKind::CustomButton(v) }
    }

    /// Build a `VPreferencesGroup` descriptor and wrap it in a `VNode`.
    pub fn preferences_group(v: VPreferencesGroup) -> Self {
        Self { key: None, kind: VNodeKind::PreferencesGroup(v) }
    }

    /// Build a `VActionRow` descriptor and wrap it in a `VNode`.
    pub fn action_row(v: VActionRow) -> Self {
        Self { key: None, kind: VNodeKind::ActionRow(v) }
    }

    /// Build a `VSwitchRow` descriptor and wrap it in a `VNode`.
    pub fn switch_row(v: VSwitchRow) -> Self {
        Self { key: None, kind: VNodeKind::SwitchRow(v) }
    }

    /// Build a `VEntryRow` descriptor and wrap it in a `VNode`.
    pub fn entry_row(v: VEntryRow) -> Self {
        Self { key: None, kind: VNodeKind::EntryRow(v) }
    }

    /// Build a `VRevealer` descriptor and wrap it in a `VNode`.
    pub fn revealer(v: VRevealer) -> Self {
        Self { key: None, kind: VNodeKind::Revealer(v) }
    }

    /// Build a `VProgressBar` descriptor and wrap it in a `VNode`.
    pub fn progress_bar(v: VProgressBar) -> Self {
        Self { key: None, kind: VNodeKind::ProgressBar(v) }
    }

    /// Build a `VScale` descriptor and wrap it in a `VNode`.
    pub fn scale(v: VScale) -> Self {
        Self { key: None, kind: VNodeKind::Scale(v) }
    }
}

// -- Internal helper ----------------------------------------------------------

fn make_component_desc<C: Component>(
    props: C::Props,
    on_output: impl Fn(C::Output) + 'static,
) -> ComponentDesc {
    let shared    = Rc::new(props);
    let on_output = Rc::new(on_output);

    let build = {
        let p  = Rc::clone(&shared);
        let cb = Rc::clone(&on_output);
        Rc::new(move || -> Box<dyn AnyWidget> {
            let comp = C::build(&*p);
            let cb   = Rc::clone(&cb);
            comp.connect_output(move |output| cb(output));
            Box::new(comp)
        }) as BuildFn
    };

    let update = {
        let p = Rc::clone(&shared);
        Rc::new(move |any: &dyn AnyWidget| {
            if let Some(comp) = any.as_any().downcast_ref::<C>() {
                comp.update(&*p);
            }
        }) as UpdateFn
    };

    let props_eq = {
        let p = Rc::clone(&shared);
        Rc::new(move |stored: &Rc<dyn Any>| {
            stored
                .downcast_ref::<C::Props>()
                .map(|old| old == &*p)
                .unwrap_or(false)
        }) as PropsEqFn
    };

    ComponentDesc {
        type_id: TypeId::of::<C>(),
        build,
        update,
        props_eq,
        props: shared,
    }
}
