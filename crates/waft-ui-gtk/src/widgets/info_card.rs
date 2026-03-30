//! Declarative info card widget.
//!
//! A card with icon, title, and optional description.
//! Layout: `[Icon 32x32] [Title (bold) / Description (dim)]`

use std::cell::RefCell;
use std::rc::Rc;

use crate::icons::Icon;
use crate::vdom::primitives::{VBox, VIcon, VLabel};
use crate::vdom::{Component, RenderCallback, RenderComponent, RenderFn, VNode};

/// Properties for the info card.
#[derive(Clone, PartialEq, Debug)]
pub struct InfoCardProps {
    pub icon: String,
    pub title: String,
    pub description: Option<String>,
}

pub struct InfoCardRender;

impl RenderFn for InfoCardRender {
    type Props = InfoCardProps;
    type Output = ();

    fn render(props: &Self::Props, _emit: &RenderCallback<()>) -> VNode {
        let icon = VIcon::new(vec![Icon::parse(&props.icon)], 32);

        let title = VLabel::new(&props.title)
            .css_class("title-3")
            .xalign(0.0);

        let mut labels_box = VBox::vertical(0)
            .valign(gtk::Align::Center)
            .child(VNode::label(title));

        // Add description label only if Some
        if let Some(desc) = &props.description {
            let description = VLabel::new(desc)
                .css_class("dim-label")
                .xalign(0.0);
            labels_box = labels_box.child(VNode::label(description));
        }

        let content_box = VBox::horizontal(8)
            .child(VNode::icon(icon))
            .child(VNode::vbox(labels_box));

        VNode::vbox(content_box)
    }
}

/// Wrapper around RenderComponent<InfoCardRender> with state tracking.
#[derive(Clone)]
pub struct InfoCardWidget {
    inner: Rc<RenderComponent<InfoCardRender>>,
    props: Rc<RefCell<InfoCardProps>>,
}

impl InfoCardWidget {
    /// Create a new info card widget (backward-compatible factory).
    pub fn new(icon: &str, title: &str, description: Option<&str>) -> Self {
        let props = InfoCardProps {
            icon: icon.to_string(),
            title: title.to_string(),
            description: description.map(std::string::ToString::to_string),
        };
        let inner = Rc::new(RenderComponent::<InfoCardRender>::build(&props));
        Self {
            inner,
            props: Rc::new(RefCell::new(props)),
        }
    }

    /// Update the icon.
    pub fn set_icon(&self, icon: &str) {
        let mut props = self.props.borrow_mut();
        props.icon = icon.to_string();
        self.inner.update(&*props);
    }

    /// Update the title text.
    pub fn set_title(&self, title: &str) {
        let mut props = self.props.borrow_mut();
        props.title = title.to_string();
        self.inner.update(&*props);
    }

    /// Update the description text and visibility.
    pub fn set_description(&self, description: Option<&str>) {
        let mut props = self.props.borrow_mut();
        props.description = description.map(std::string::ToString::to_string);
        self.inner.update(&*props);
    }

    /// Get a reference to the root widget.
    pub fn widget(&self) -> gtk::Widget {
        self.inner.widget()
    }
}

impl crate::widget_base::WidgetBase for InfoCardWidget {
    fn widget(&self) -> gtk::Widget {
        self.widget()
    }
}
