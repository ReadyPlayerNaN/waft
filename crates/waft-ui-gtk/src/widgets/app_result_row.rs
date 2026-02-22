//! AppResultRowWidget -- dumb row for a single app search result.

use crate::icons::Icon;
use crate::vdom::{RenderCallback, RenderComponent, RenderFn, VBox, VIcon, VLabel, VNode};

/// Properties for an app result row.
#[derive(Clone, PartialEq)]
pub struct AppResultRowProps {
    pub name: String,
    pub icon: String,
    pub description: Option<String>,
}

/// Renders a horizontal row: 48px icon + vertical label stack (name + optional description).
///
/// No Output enum -- selection and activation are handled at the list level.
pub struct AppResultRowRender;

impl RenderFn for AppResultRowRender {
    type Props = AppResultRowProps;
    type Output = ();

    fn render(props: &Self::Props, _emit: &RenderCallback<()>) -> VNode {
        let mut label_box = VBox::vertical(2)
            .valign(gtk::Align::Center)
            .child(VNode::label(
                VLabel::new(&props.name)
                    .css_class("app-result-name")
                    .xalign(0.0),
            ));

        if let Some(ref desc) = props.description {
            label_box = label_box.child(VNode::label(
                VLabel::new(desc)
                    .css_class("app-result-description")
                    .css_class("dim-label")
                    .ellipsize(gtk::pango::EllipsizeMode::End)
                    .xalign(0.0),
            ));
        }

        VNode::vbox(
            VBox::horizontal(12)
                .css_class("app-result-row")
                .child(VNode::icon(VIcon::new(
                    vec![Icon::Themed(props.icon.clone())],
                    48,
                )))
                .child(VNode::vbox(label_box)),
        )
    }
}

/// Type alias preserving the old name for callers.
pub type AppResultRowWidget = RenderComponent<AppResultRowRender>;
