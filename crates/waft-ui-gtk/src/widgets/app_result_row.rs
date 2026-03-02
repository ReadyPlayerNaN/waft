//! AppResultRowWidget -- dumb row for a single app search result.

use crate::icons::Icon;
use crate::vdom::{RenderCallback, RenderComponent, RenderFn, VBox, VIcon, VLabel, VNode};

/// Whether the result is an application or a compositor window.
#[derive(Clone, PartialEq)]
pub enum ResultKind {
    App,
    Window,
}

/// Properties for an app result row.
#[derive(Clone, PartialEq)]
pub struct AppResultRowProps {
    pub name: String,
    pub icon: String,
    pub kind: ResultKind,
}

/// Renders a horizontal row: badge + 24px icon + wrapping name label.
///
/// No Output enum -- selection and activation are handled at the list level.
pub struct AppResultRowRender;

impl RenderFn for AppResultRowRender {
    type Props = AppResultRowProps;
    type Output = ();

    fn render(props: &Self::Props, _emit: &RenderCallback<()>) -> VNode {
        let (badge_text, badge_modifier) = match props.kind {
            ResultKind::App => ("A", "badge-app"),
            ResultKind::Window => ("W", "badge-window"),
        };

        VNode::vbox(
            VBox::horizontal(8)
                .css_class("app-result-row")
                .child(VNode::label(
                    VLabel::new(badge_text)
                        .css_class("app-result-badge")
                        .css_class(badge_modifier),
                ))
                .child(VNode::icon(VIcon::new(
                    vec![Icon::Themed(props.icon.clone())],
                    24,
                )))
                .child(VNode::label(
                    VLabel::new(&props.name)
                        .css_class("app-result-name")
                        .xalign(0.0)
                        .wrap(true)
                        .wrap_mode(gtk::pango::WrapMode::WordChar),
                )),
        )
    }
}

/// Type alias preserving the old name for callers.
pub type AppResultRowWidget = RenderComponent<AppResultRowRender>;
