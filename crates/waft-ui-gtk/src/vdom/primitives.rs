use std::rc::Rc;

use crate::icons::Icon;

/// Descriptor for a `gtk::Label` primitive VNode.
pub struct VLabel {
    pub text:        String,
    pub css_classes: Vec<String>,
    pub xalign:      Option<f32>,
    pub hexpand:     bool,
    pub ellipsize:   Option<gtk::pango::EllipsizeMode>,
}

impl VLabel {
    pub fn new(text: impl Into<String>) -> Self {
        Self {
            text:        text.into(),
            css_classes: Vec::new(),
            xalign:      None,
            hexpand:     false,
            ellipsize:   None,
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
    pub hints:      Vec<Icon>,
    pub pixel_size: i32,
    pub visible:    bool,
}

impl VIcon {
    pub fn new(hints: Vec<Icon>, pixel_size: i32) -> Self {
        Self { hints, pixel_size, visible: true }
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
