use std::rc::Rc;

use crate::icons::Icon;

/// Descriptor for a `gtk::Button` with a `VNode` child tree (not a text label).
///
/// The button's content is a `VNode` reconciled inside a `gtk::Box` child
/// container. Use `VButton` instead when all you need is a text label.
pub struct VCustomButton {
    pub child:       Box<super::VNode>,
    pub css_classes: Vec<String>,
    pub visible:     bool,
    pub sensitive:   bool,
    pub on_click:    Option<Rc<dyn Fn()>>,
}

impl VCustomButton {
    pub fn new(child: super::VNode) -> Self {
        Self {
            child:       Box::new(child),
            css_classes: Vec::new(),
            visible:     true,
            sensitive:   true,
            on_click:    None,
        }
    }

    pub fn css_class(mut self, class: impl Into<String>) -> Self {
        self.css_classes.push(class.into());
        self
    }

    pub fn css_classes(mut self, classes: impl IntoIterator<Item = impl Into<String>>) -> Self {
        self.css_classes.extend(classes.into_iter().map(|c| c.into()));
        self
    }

    pub fn visible(mut self, v: bool) -> Self {
        self.visible = v;
        self
    }

    pub fn sensitive(mut self, v: bool) -> Self {
        self.sensitive = v;
        self
    }

    pub fn on_click(mut self, f: impl Fn() + 'static) -> Self {
        self.on_click = Some(Rc::new(f));
        self
    }
}

/// Descriptor for a `gtk::Label` primitive VNode.
pub struct VLabel {
    pub text:        String,
    pub css_classes: Vec<String>,
    pub xalign:      Option<f32>,
    pub hexpand:     bool,
    pub ellipsize:   Option<gtk::pango::EllipsizeMode>,
    pub wrap:        bool,
    pub wrap_mode:   Option<gtk::pango::WrapMode>,
}

impl VLabel {
    pub fn new(text: impl Into<String>) -> Self {
        Self {
            text:        text.into(),
            css_classes: Vec::new(),
            xalign:      None,
            hexpand:     false,
            ellipsize:   None,
            wrap:        false,
            wrap_mode:   None,
        }
    }

    pub fn css_class(mut self, class: impl Into<String>) -> Self {
        self.css_classes.push(class.into());
        self
    }

    pub fn xalign(mut self, x: f32) -> Self {
        self.xalign = Some(x);
        self
    }

    pub fn hexpand(mut self, v: bool) -> Self {
        self.hexpand = v;
        self
    }

    pub fn ellipsize(mut self, mode: gtk::pango::EllipsizeMode) -> Self {
        self.ellipsize = Some(mode);
        self
    }

    pub fn wrap(mut self, v: bool) -> Self {
        self.wrap = v;
        self
    }

    pub fn wrap_mode(mut self, mode: gtk::pango::WrapMode) -> Self {
        self.wrap_mode = Some(mode);
        self
    }
}

/// Descriptor for a `gtk::Box` container with child VNodes.
pub struct VBox {
    pub orientation: gtk::Orientation,
    pub spacing:     i32,
    pub css_classes: Vec<String>,
    /// Child VNodes. Reconciled by a child `Reconciler` inside the live entry.
    pub children:    Vec<super::VNode>,
    pub valign:      Option<gtk::Align>,
    pub halign:      Option<gtk::Align>,
}

impl VBox {
    pub fn horizontal(spacing: i32) -> Self {
        Self {
            orientation: gtk::Orientation::Horizontal,
            spacing,
            css_classes: Vec::new(),
            children:    Vec::new(),
            valign:      None,
            halign:      None,
        }
    }

    pub fn vertical(spacing: i32) -> Self {
        Self {
            orientation: gtk::Orientation::Vertical,
            spacing,
            css_classes: Vec::new(),
            children:    Vec::new(),
            valign:      None,
            halign:      None,
        }
    }

    pub fn css_class(mut self, class: impl Into<String>) -> Self {
        self.css_classes.push(class.into());
        self
    }

    pub fn child(mut self, node: super::VNode) -> Self {
        self.children.push(node);
        self
    }

    pub fn valign(mut self, a: gtk::Align) -> Self {
        self.valign = Some(a);
        self
    }

    pub fn halign(mut self, a: gtk::Align) -> Self {
        self.halign = Some(a);
        self
    }
}

/// Descriptor for a `gtk::Button` primitive VNode.
pub struct VButton {
    pub label:    String,
    pub sensitive: bool,
    /// Callback reconnected on every update (closures have no identity).
    pub on_click: Option<Rc<dyn Fn()>>,
}

impl VButton {
    pub fn new(label: impl Into<String>) -> Self {
        Self { label: label.into(), sensitive: true, on_click: None }
    }

    pub fn sensitive(mut self, v: bool) -> Self {
        self.sensitive = v;
        self
    }

    pub fn on_click(mut self, f: impl Fn() + 'static) -> Self {
        self.on_click = Some(Rc::new(f));
        self
    }
}

/// Descriptor for an `IconWidget` primitive VNode.
pub struct VIcon {
    pub hints:       Vec<Icon>,
    pub pixel_size:  i32,
    pub visible:     bool,
    pub fallback:    bool,
    pub css_classes: Vec<String>,
}

impl VIcon {
    pub fn new(hints: Vec<Icon>, pixel_size: i32) -> Self {
        Self { hints, pixel_size, visible: true, fallback: true, css_classes: Vec::new() }
    }

    pub fn visible(mut self, v: bool) -> Self {
        self.visible = v;
        self
    }

    pub fn fallback(mut self, v: bool) -> Self {
        self.fallback = v;
        self
    }

    pub fn css_class(mut self, class: impl Into<String>) -> Self {
        self.css_classes.push(class.into());
        self
    }

    pub fn css_classes(mut self, classes: impl IntoIterator<Item = impl Into<String>>) -> Self {
        self.css_classes.extend(classes.into_iter().map(|c| c.into()));
        self
    }
}

/// Descriptor for a `gtk::ProgressBar` primitive VNode.
pub struct VProgressBar {
    pub fraction:    f64,
    pub css_classes: Vec<String>,
    pub visible:     bool,
}

impl VProgressBar {
    pub fn new(fraction: f64) -> Self {
        Self { fraction, css_classes: Vec::new(), visible: true }
    }

    pub fn css_class(mut self, class: impl Into<String>) -> Self {
        self.css_classes.push(class.into());
        self
    }

    pub fn css_class_if(self, condition: bool, class: impl Into<String>) -> Self {
        if condition { self.css_class(class) } else { self }
    }

    pub fn visible(mut self, v: bool) -> Self {
        self.visible = v;
        self
    }
}

/// Descriptor for a `gtk::Spinner` primitive VNode.
pub struct VSpinner {
    pub spinning: bool,
    pub visible:  bool,
}

impl VSpinner {
    pub fn new(spinning: bool) -> Self {
        Self { spinning, visible: spinning }
    }

    pub fn visible(mut self, v: bool) -> Self {
        self.visible = v;
        self
    }
}

/// Descriptor for an `adw::PreferencesGroup` container VNode.
pub struct VPreferencesGroup {
    pub title:    Option<String>,
    pub children: Vec<super::VNode>,
}

impl VPreferencesGroup {
    pub fn new() -> Self {
        Self { title: None, children: Vec::new() }
    }

    pub fn title(mut self, t: impl Into<String>) -> Self {
        self.title = Some(t.into());
        self
    }

    pub fn child(mut self, node: super::VNode) -> Self {
        self.children.push(node);
        self
    }
}

impl Default for VPreferencesGroup {
    fn default() -> Self {
        Self::new()
    }
}

/// Descriptor for an `adw::ActionRow` VNode.
pub struct VActionRow {
    pub title:       String,
    pub subtitle:    Option<String>,
    pub suffix:      Vec<super::VNode>,
    pub prefix:      Vec<super::VNode>,
    pub activatable: bool,
    pub on_activate: Option<Rc<dyn Fn()>>,
}

impl VActionRow {
    pub fn new(title: impl Into<String>) -> Self {
        Self {
            title:       title.into(),
            subtitle:    None,
            suffix:      Vec::new(),
            prefix:      Vec::new(),
            activatable: false,
            on_activate: None,
        }
    }

    pub fn subtitle(mut self, s: impl Into<String>) -> Self {
        self.subtitle = Some(s.into());
        self
    }

    pub fn suffix(mut self, node: super::VNode) -> Self {
        self.suffix.push(node);
        self
    }

    pub fn prefix(mut self, node: super::VNode) -> Self {
        self.prefix.push(node);
        self
    }

    pub fn on_activate(mut self, f: impl Fn() + 'static) -> Self {
        self.activatable = true;
        self.on_activate = Some(Rc::new(f));
        self
    }
}

/// Descriptor for an `adw::SwitchRow` VNode.
pub struct VSwitchRow {
    pub title:     String,
    pub subtitle:  Option<String>,
    pub active:    bool,
    pub sensitive: bool,
    pub on_toggle: Option<Rc<dyn Fn(bool)>>,
}

impl VSwitchRow {
    pub fn new(title: impl Into<String>, active: bool) -> Self {
        Self {
            title:     title.into(),
            subtitle:  None,
            active,
            sensitive: true,
            on_toggle: None,
        }
    }

    pub fn subtitle(mut self, s: impl Into<String>) -> Self {
        self.subtitle = Some(s.into());
        self
    }

    pub fn sensitive(mut self, v: bool) -> Self {
        self.sensitive = v;
        self
    }

    pub fn on_toggle(mut self, f: impl Fn(bool) + 'static) -> Self {
        self.on_toggle = Some(Rc::new(f));
        self
    }
}

/// Descriptor for an `adw::EntryRow` VNode.
pub struct VEntryRow {
    pub title:     String,
    pub text:      String,
    pub sensitive: bool,
    pub on_change: Option<Rc<dyn Fn(String)>>,
}

impl VEntryRow {
    pub fn new(title: impl Into<String>) -> Self {
        Self {
            title:     title.into(),
            text:      String::new(),
            sensitive: true,
            on_change: None,
        }
    }

    pub fn text(mut self, t: impl Into<String>) -> Self {
        self.text = t.into();
        self
    }

    pub fn sensitive(mut self, v: bool) -> Self {
        self.sensitive = v;
        self
    }

    pub fn on_change(mut self, f: impl Fn(String) + 'static) -> Self {
        self.on_change = Some(Rc::new(f));
        self
    }
}

/// Descriptor for a `gtk::Revealer` container VNode.
pub struct VRevealer {
    pub reveal:              bool,
    pub transition_type:     gtk::RevealerTransitionType,
    pub transition_duration: u32,
    pub child:               Box<super::VNode>,
}

impl VRevealer {
    pub fn new(reveal: bool, child: super::VNode) -> Self {
        Self {
            reveal,
            transition_type: gtk::RevealerTransitionType::SlideDown,
            transition_duration: 200,
            child: Box::new(child),
        }
    }

    pub fn transition_type(mut self, t: gtk::RevealerTransitionType) -> Self {
        self.transition_type = t;
        self
    }

    pub fn transition_duration(mut self, ms: u32) -> Self {
        self.transition_duration = ms;
        self
    }
}

/// Descriptor for a `gtk::Scale` primitive VNode with interaction tracking.
///
/// The reconciler entry manages gesture controllers, signal blocking, and
/// debounce timers. During active user interaction (drag, scroll, keyboard),
/// backend value updates are suppressed to avoid fighting the user.
pub struct VScale {
    pub value:           f64,
    pub css_classes:     Vec<String>,
    pub on_value_change: Option<Rc<dyn Fn(f64)>>,
    pub on_value_commit: Option<Rc<dyn Fn(f64)>>,
}

impl VScale {
    pub fn new(value: f64) -> Self {
        Self {
            value,
            css_classes: Vec::new(),
            on_value_change: None,
            on_value_commit: None,
        }
    }

    pub fn css_class(mut self, class: impl Into<String>) -> Self {
        self.css_classes.push(class.into());
        self
    }

    pub fn on_value_change(mut self, f: impl Fn(f64) + 'static) -> Self {
        self.on_value_change = Some(Rc::new(f));
        self
    }

    pub fn on_value_commit(mut self, f: impl Fn(f64) + 'static) -> Self {
        self.on_value_commit = Some(Rc::new(f));
        self
    }
}

/// Descriptor for a `gtk::Switch` primitive VNode.
pub struct VSwitch {
    pub active:      bool,
    pub sensitive:   bool,
    pub css_classes: Vec<String>,
    /// Callback reconnected on every update.
    pub on_toggle:   Option<Rc<dyn Fn(bool)>>,
}

impl VSwitch {
    pub fn new(active: bool) -> Self {
        Self { active, sensitive: true, css_classes: Vec::new(), on_toggle: None }
    }

    pub fn sensitive(mut self, v: bool) -> Self {
        self.sensitive = v;
        self
    }

    pub fn css_class(mut self, class: impl Into<String>) -> Self {
        self.css_classes.push(class.into());
        self
    }

    pub fn on_toggle(mut self, f: impl Fn(bool) + 'static) -> Self {
        self.on_toggle = Some(Rc::new(f));
        self
    }
}
