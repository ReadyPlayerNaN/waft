//! Slider widget — icon button, horizontal scale, and optional expand button.
//!
//! Rendered as a pure `RenderFn`. The VScale primitive in the vdom reconciler
//! handles interaction tracking (gesture detection, signal blocking, debounce).

use crate::icons::Icon;
use crate::vdom::{RenderCallback, RenderFn, VBox, VCustomButton, VIcon, VNode, VRevealer, VScale};
use crate::widgets::menu_chevron::{MenuChevronProps, MenuChevronWidget};

/// Properties for rendering a slider.
#[derive(Clone, PartialEq, Debug)]
pub struct SliderRenderProps {
    pub icon: String,
    pub value: f64,
    pub disabled: bool,
    pub expandable: bool,
    pub expanded: bool,
}

/// Output events from the slider render.
#[derive(Debug, Clone)]
pub enum SliderRenderOutput {
    ValueChanged(f64),
    ValueCommit(f64),
    IconClick,
    ExpandClick,
}

/// Pure render function for the slider widget.
pub struct SliderRender;

impl RenderFn for SliderRender {
    type Props = SliderRenderProps;
    type Output = SliderRenderOutput;

    fn render(props: &Self::Props, emit: &RenderCallback<SliderRenderOutput>) -> VNode {
        // Icon button
        let emit_icon = emit.clone();
        let icon_button = VNode::custom_button(
            VCustomButton::new(VNode::icon(VIcon::new(
                vec![Icon::Themed(props.icon.clone())],
                24,
            )))
            .css_class("slider-icon")
            .on_click(move || {
                if let Some(ref cb) = *emit_icon.borrow() {
                    cb(SliderRenderOutput::IconClick);
                }
            }),
        );

        // Scale
        let emit_vc = emit.clone();
        let emit_commit = emit.clone();
        let scale = VNode::scale(
            VScale::new(props.value)
                .css_class("slider-scale")
                .on_value_change(move |v| {
                    if let Some(ref cb) = *emit_vc.borrow() {
                        cb(SliderRenderOutput::ValueChanged(v));
                    }
                })
                .on_value_commit(move |v| {
                    if let Some(ref cb) = *emit_commit.borrow() {
                        cb(SliderRenderOutput::ValueCommit(v));
                    }
                }),
        );

        println!("Slider {props:?}");

        // Expand button with chevron icon (inside a revealer)
        let emit_expand = emit.clone();
        let expand_button = VNode::custom_button(
            VCustomButton::new(VNode::new::<MenuChevronWidget>(MenuChevronProps {
                expanded: props.expanded,
            }))
            .css_class("slider-expand")
            .on_click(move || {
                if let Some(ref cb) = *emit_expand.borrow() {
                    cb(SliderRenderOutput::ExpandClick);
                }
            }),
        );

        let expand_revealer = VNode::revealer(
            VRevealer::new(props.expandable, expand_button)
                .transition_type(gtk::RevealerTransitionType::SlideLeft)
                .transition_duration(200),
        );

        // Controls box: icon + scale + expand
        let controls = VNode::vbox(
            VBox::horizontal(8)
                .child(icon_button)
                .child(scale)
                .child(expand_revealer),
        );

        // Root box
        let mut root = VBox::vertical(0).css_class("slider-row");
        if props.disabled {
            root = root.css_class("disabled");
        }
        VNode::vbox(root.child(controls))
    }
}

/// Type alias for the slider component.
pub type SliderWidget = crate::vdom::RenderComponent<SliderRender>;
