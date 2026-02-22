use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;

use waft_core::Callback;
use waft_ui_gtk::icons::Icon;
use waft_ui_gtk::vdom::Component;

use crate::ui::feature_toggles::menu_button::FeatureToggleMenuButtonOutput;

use super::menu_button::{FeatureToggleMenuButton, FeatureToggleMenuButtonProps};

#[derive(Clone, PartialEq)]
pub struct FeatureToggleMenuSettingsButtonProps {
    pub label: String,
    pub visible: bool,
}

pub struct FeatureToggleMenuSettingsButton {
    root: FeatureToggleMenuButton,
    on_output: Callback<FeatureToggleMenuButtonOutput>,
}

impl Component for FeatureToggleMenuSettingsButton {
    type Props = FeatureToggleMenuSettingsButtonProps;
    type Output = FeatureToggleMenuButtonOutput;

    fn build(props: &Self::Props) -> Self {
        let root = FeatureToggleMenuButton::new(FeatureToggleMenuButtonProps {
            disabled: false,
            name: props.label.clone(),
            working: false,
        });

        root.set_primary_icon(vec![Icon::parse(&Arc::from("preferences-system-symbolic"))]);
        root.set_visible(props.visible);

        let on_output: Callback<FeatureToggleMenuButtonOutput> = Rc::new(RefCell::new(None));
        let on_output_ref = on_output.clone();
        root.connect_output(move |_| {
            if let Some(ref callback) = *on_output_ref.borrow() {
                callback(FeatureToggleMenuButtonOutput::Click);
            }
        });

        Self { root, on_output }
    }

    fn update(&self, props: &Self::Props) {
        self.root.set_name(&props.label);
        self.root.set_visible(props.visible);
    }

    fn connect_output<F: Fn(Self::Output) + 'static>(&self, callback: F) {
        *self.on_output.borrow_mut() = Some(Box::new(callback));
    }

    fn widget(&self) -> gtk::Widget {
        self.root.widget()
    }
}
