use waft_ui_gtk::icons::Icon;
use waft_ui_gtk::vdom::{RenderCallback, RenderComponent, RenderFn, VNode};

use super::menu_button::{
    FeatureToggleMenuButton, FeatureToggleMenuButtonOutput, FeatureToggleMenuButtonProps,
};

#[derive(Clone, PartialEq)]
pub struct FeatureToggleMenuSettingsButtonProps {
    pub label:   String,
    pub visible: bool,
}

pub(crate) struct FeatureToggleMenuSettingsButtonRender;

impl RenderFn for FeatureToggleMenuSettingsButtonRender {
    type Props  = FeatureToggleMenuSettingsButtonProps;
    type Output = FeatureToggleMenuButtonOutput;

    fn render(props: &Self::Props, emit: &RenderCallback<Self::Output>) -> VNode {
        let emit_clone = emit.clone();
        VNode::with_output::<FeatureToggleMenuButton>(
            FeatureToggleMenuButtonProps {
                disabled:       false,
                name:           props.label.clone(),
                working:        false,
                primary_icon:   vec![Icon::parse("preferences-system-symbolic")],
                secondary_icon: vec![],
                visible:        props.visible,
                switch_active:  None,
            },
            move |click| {
                if let Some(ref cb) = *emit_clone.borrow() {
                    cb(click);
                }
            },
        )
    }
}

pub type FeatureToggleMenuSettingsButton = RenderComponent<FeatureToggleMenuSettingsButtonRender>;
