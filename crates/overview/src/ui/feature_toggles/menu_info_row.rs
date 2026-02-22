use waft_ui_gtk::vdom::{RenderCallback, RenderComponent, RenderFn, VNode};
use waft_ui_gtk::vdom::primitives::{VBox, VLabel};

#[derive(Clone, PartialEq)]
pub struct FeatureToggleMenuInfoRowProps {
    pub label: String,
    pub value: String,
}

// ── Render function ───────────────────────────────────────────────────────

pub(crate) struct InfoRowRender;

impl RenderFn for InfoRowRender {
    type Props  = FeatureToggleMenuInfoRowProps;
    type Output = ();

    fn render(props: &Self::Props, _emit: &RenderCallback<()>) -> VNode {
        VNode::vbox(
            VBox::horizontal(12)
                .css_class("menu-row")
                .child(VNode::vbox(
                    VBox::horizontal(12)
                        .css_class("menu-row")
                        .child(VNode::label(
                            VLabel::new(&props.label)
                                .css_class("dim-label")
                                .xalign(0.0),
                        )),
                ))
                .child(VNode::vbox(
                    VBox::horizontal(12)
                        .css_class("menu-row")
                        .child(VNode::label(
                            VLabel::new(&props.value)
                                .hexpand(true)
                                .xalign(1.0),
                        )),
                )),
        )
    }
}

// ── Public type alias ─────────────────────────────────────────────────────

/// Per-row info display showing a label-value pair in the feature toggle menu.
///
/// Migrated to `RenderFn` — no stored widget references, layout described
/// declaratively in `InfoRowRender::render()`.
pub type FeatureToggleMenuInfoRow = RenderComponent<InfoRowRender>;
