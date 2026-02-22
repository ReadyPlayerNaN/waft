use std::rc::Rc;

/// Descriptor for a `gtk::Label` primitive VNode.
pub struct VLabel {
    pub text:        String,
    pub css_classes: Vec<String>,
    pub xalign:      Option<f32>,
    pub hexpand:     bool,
}

impl VLabel {
    pub fn new(text: impl Into<String>) -> Self {
        Self {
            text:        text.into(),
            css_classes: Vec::new(),
            xalign:      None,
            hexpand:     false,
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
}

/// Descriptor for a `gtk::Box` container with child VNodes.
pub struct VBox {
    pub orientation: gtk::Orientation,
    pub spacing:     i32,
    pub css_classes: Vec<String>,
    /// Child VNodes. Reconciled by a child `Reconciler` inside the live entry.
    pub children:    Vec<super::VNode>,
}

impl VBox {
    pub fn horizontal(spacing: i32) -> Self {
        Self {
            orientation: gtk::Orientation::Horizontal,
            spacing,
            css_classes: Vec::new(),
            children:    Vec::new(),
        }
    }

    pub fn vertical(spacing: i32) -> Self {
        Self {
            orientation: gtk::Orientation::Vertical,
            spacing,
            css_classes: Vec::new(),
            children:    Vec::new(),
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

/// Descriptor for a `gtk::Switch` primitive VNode.
pub struct VSwitch {
    pub active:    bool,
    pub sensitive: bool,
    /// Callback reconnected on every update.
    pub on_toggle: Option<Rc<dyn Fn(bool)>>,
}

impl VSwitch {
    pub fn new(active: bool) -> Self {
        Self { active, sensitive: true, on_toggle: None }
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
