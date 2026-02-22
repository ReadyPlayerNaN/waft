//! Declarative feature toggle widget.
//!
//! A unified toggle button that can be simple or expandable.
//! When expandable=false, only shows the main toggle button.
//! When expandable=true, shows both main button and expand button with menu support.

use std::cell::RefCell;
use std::rc::Rc;

use crate::icons::Icon;
use crate::vdom::primitives::{VBox, VCustomButton, VIcon, VLabel, VRevealer};
use crate::vdom::{Component, RenderCallback, RenderComponent, RenderFn, VNode};
use crate::widgets::menu_chevron::{MenuChevronProps, MenuChevronWidget};

/// Properties for rendering a feature toggle.
#[derive(Clone, PartialEq, Debug)]
pub struct FeatureToggleProps {
    pub active: bool,
    pub busy: bool,
    pub details: Option<String>,
    pub expandable: bool,
    pub icon: String,
    pub title: String,
    /// Optional deterministic menu ID. When provided, the toggle uses this
    /// instead of generating a random UUID. Callers should use
    /// `menu_id_for_widget(widget_id)` to produce a stable ID that
    /// matches any external content revealer.
    pub menu_id: Option<String>,
    /// Expanded state: tracks if the menu is open.
    pub expanded: bool,
}

/// Output events from the feature toggle.
#[derive(Debug, Clone)]
pub enum FeatureToggleOutput {
    Activate,
    Deactivate,
    ExpandToggle(bool),
}

/// Pure render function for the feature toggle widget.
pub struct FeatureToggleRender;

impl RenderFn for FeatureToggleRender {
    type Props = FeatureToggleProps;
    type Output = FeatureToggleOutput;

    fn render(props: &Self::Props, emit: &RenderCallback<Self::Output>) -> VNode {
        // Title label
        let title = VLabel::new(&props.title)
            .css_class("heading")
            .css_class("title")
            .xalign(0.0);

        // Build text content: title + optional details
        let mut text_box = VBox::vertical(2).valign(gtk::Align::Center);
        text_box = text_box.child(VNode::label(title));

        // Details label (inside revealer)
        let details_revealer = if let Some(ref details) = props.details {
            let details_label = VLabel::new(details)
                .css_class("dim-label")
                .css_class("caption")
                .xalign(0.0);
            VNode::revealer(
                VRevealer::new(true, VNode::label(details_label))
                    .transition_type(gtk::RevealerTransitionType::SlideDown),
            )
        } else {
            let empty_label = VLabel::new("")
                .css_class("dim-label")
                .css_class("caption")
                .xalign(0.0);
            VNode::revealer(
                VRevealer::new(false, VNode::label(empty_label))
                    .transition_type(gtk::RevealerTransitionType::SlideDown),
            )
        };
        text_box = text_box.child(details_revealer);

        // Main button content: icon + text_box
        let icon = VIcon::new(vec![Icon::Themed(props.icon.clone())], 24);
        let main_content = VBox::horizontal(12)
            .valign(gtk::Align::Center)
            .child(VNode::icon(icon))
            .child(VNode::vbox(text_box));

        // Main button: click emits Activate/Deactivate based on current state
        let emit_main = emit.clone();
        let is_active = props.active;
        let main_button = VNode::custom_button(
            VCustomButton::new(VNode::vbox(main_content))
                .css_class("toggle-main")
                .on_click(move || {
                    if let Some(ref cb) = *emit_main.borrow() {
                        if is_active {
                            cb(FeatureToggleOutput::Deactivate);
                        } else {
                            cb(FeatureToggleOutput::Activate);
                        }
                    }
                }),
        );

        // Expand button with MenuChevronWidget child
        let emit_expand = emit.clone();
        let expand_chevron = VNode::new::<MenuChevronWidget>(MenuChevronProps {
            expanded: props.expanded,
        });
        let expand_button = VNode::custom_button(
            VCustomButton::new(expand_chevron)
                .css_class("toggle-expand")
                .on_click(move || {
                    if let Some(ref cb) = *emit_expand.borrow() {
                        cb(FeatureToggleOutput::ExpandToggle(true));
                    }
                }),
        );

        // Wrap expand button in revealer with slide-left transition
        let expand_revealer = VNode::revealer(
            VRevealer::new(props.expandable, expand_button)
                .transition_type(gtk::RevealerTransitionType::SlideLeft)
                .transition_duration(200),
        );

        // Root box: main button + expand revealer (horizontal layout)
        let mut root = VBox::horizontal(0).css_class("feature-toggle");

        // Apply state CSS classes
        if props.active {
            root = root.css_class("active");
        }
        if props.busy {
            root = root.css_class("busy");
        }
        if props.expandable {
            root = root.css_class("expandable");
        }
        if props.expanded {
            root = root.css_class("expanded");
        }

        let root = root.child(main_button).child(expand_revealer);

        VNode::vbox(root)
    }
}

/// Wrapper around RenderComponent<FeatureToggleRender> with backward-compatible API.
#[derive(Clone)]
pub struct FeatureToggleWidget {
    inner: Rc<RenderComponent<FeatureToggleRender>>,
    props: Rc<RefCell<FeatureToggleProps>>,
    pub menu_id: Option<String>,
}

impl FeatureToggleWidget {
    /// Create a new feature toggle widget (backward-compatible factory).
    pub fn new(props: FeatureToggleProps, _menu_store: Option<std::rc::Rc<waft_core::menu_state::MenuStore>>) -> Self {
        let menu_id = props.menu_id.clone();
        let props_with_expanded = FeatureToggleProps {
            expanded: false,
            ..props
        };
        let inner = Rc::new(RenderComponent::<FeatureToggleRender>::build(&props_with_expanded));
        Self {
            inner,
            props: Rc::new(RefCell::new(props_with_expanded)),
            menu_id,
        }
    }

    /// Set the callback for output events.
    pub fn connect_output<F>(&self, callback: F)
    where
        F: Fn(FeatureToggleOutput) + 'static,
    {
        self.inner.connect_output(callback);
    }

    /// Update the active state.
    pub fn set_active(&self, active: bool) {
        let mut props = self.props.borrow_mut();
        if props.active != active {
            props.active = active;
            self.inner.update(&*props);
        }
    }

    /// Update the busy state.
    pub fn set_busy(&self, busy: bool) {
        let mut props = self.props.borrow_mut();
        if props.busy != busy {
            props.busy = busy;
            self.inner.update(&*props);
        }
    }

    /// Update the expandable state.
    pub fn set_expandable(&self, expandable: bool) {
        let mut props = self.props.borrow_mut();
        if props.expandable != expandable {
            props.expandable = expandable;
            self.inner.update(&*props);
        }
    }

    /// Update the details text.
    pub fn set_details(&self, details: Option<String>) {
        let mut props = self.props.borrow_mut();
        if props.details != details {
            props.details = details;
            self.inner.update(&*props);
        }
    }

    /// Update the icon.
    pub fn set_icon(&self, icon: &str) {
        let mut props = self.props.borrow_mut();
        if props.icon != icon {
            props.icon = icon.to_string();
            self.inner.update(&*props);
        }
    }

    /// Update the title text.
    pub fn set_title(&self, title: &str) {
        let mut props = self.props.borrow_mut();
        if props.title != title {
            props.title = title.to_string();
            self.inner.update(&*props);
        }
    }

    /// Update the expanded state (called by parent when MenuStore changes).
    pub fn set_expanded(&self, expanded: bool) {
        let mut props = self.props.borrow_mut();
        if props.expanded != expanded {
            props.expanded = expanded;
            self.inner.update(&*props);
        }
    }

    /// Get a reference to the root widget.
    pub fn widget(&self) -> gtk::Widget {
        self.inner.widget()
    }
}

impl crate::widget_base::WidgetBase for FeatureToggleWidget {
    fn widget(&self) -> gtk::Widget {
        self.widget()
    }
}
