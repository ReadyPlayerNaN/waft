use waft_ui_gtk::icons::Icon;
use waft_ui_gtk::vdom::{RenderCallback, RenderComponent, RenderFn, VNode};
use waft_ui_gtk::vdom::primitives::{VBox, VCustomButton, VIcon, VLabel, VSpinner, VSwitch};

#[derive(Clone, PartialEq)]
pub struct FeatureToggleMenuButtonProps {
    pub disabled:       bool,
    pub name:           String,
    pub working:        bool,
    pub primary_icon:   Vec<Icon>,
    pub secondary_icon: Vec<Icon>,
    pub visible:        bool,
    /// When `Some`, renders a non-interactive display-only switch in the right slot.
    pub switch_active:  Option<bool>,
}

pub enum FeatureToggleMenuButtonOutput {
    Click,
}

struct FeatureToggleMenuButtonRender;

impl RenderFn for FeatureToggleMenuButtonRender {
    type Props  = FeatureToggleMenuButtonProps;
    type Output = FeatureToggleMenuButtonOutput;

    fn render(props: &Self::Props, emit: &RenderCallback<Self::Output>) -> VNode {
        let emit_clone = emit.clone();

        let icon_box = VBox::horizontal(4)
            .valign(gtk::Align::Center)
            .child(VNode::icon(
                VIcon::new(props.primary_icon.clone(), 16)
                    .visible(!props.primary_icon.is_empty()),
            ))
            .child(VNode::icon(
                VIcon::new(props.secondary_icon.clone(), 16)
                    .visible(!props.secondary_icon.is_empty()),
            ));

        let mut right_box = VBox::horizontal(4).valign(gtk::Align::Center);
        if let Some(active) = props.switch_active {
            right_box = right_box.child(VNode::switch(
                VSwitch::new(active)
                    .sensitive(false)
                    .css_class("device-switch"),
            ));
        }

        let inner = VBox::horizontal(8)
            .child(VNode::vbox(icon_box))
            .child(VNode::label(
                VLabel::new(&props.name)
                    .hexpand(true)
                    .xalign(0.0)
                    .ellipsize(gtk::pango::EllipsizeMode::End),
            ))
            .child(VNode::spinner(
                VSpinner::new(props.working).visible(props.working),
            ))
            .child(VNode::vbox(right_box));

        VNode::custom_button(
            VCustomButton::new(VNode::vbox(inner))
                .css_classes(["flat", "device-row"])
                .sensitive(!props.disabled && !props.working)
                .visible(props.visible)
                .on_click(move || {
                    if let Some(ref cb) = *emit_clone.borrow() {
                        cb(FeatureToggleMenuButtonOutput::Click);
                    }
                }),
        )
    }
}

pub type FeatureToggleMenuButton = RenderComponent<FeatureToggleMenuButtonRender>;
