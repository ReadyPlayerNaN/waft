use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;

use waft_core::Callback;
use waft_ui_gtk::icons::Icon;

use crate::ui::feature_toggles::menu_button::FeatureToggleMenuButtonOutput;

use super::menu_button::{FeatureToggleMenuButton, FeatureToggleMenuButtonProps};

pub struct FeatureToggleMenuSettingsButtonProps {
    pub label: String,
}

pub struct FeatureToggleMenuSettingsButton {
    root: FeatureToggleMenuButton,
    on_output: Callback<FeatureToggleMenuButtonOutput>,
}

impl FeatureToggleMenuSettingsButton {
    pub fn new(props: FeatureToggleMenuSettingsButtonProps) -> Self {
        let root = FeatureToggleMenuButton::new(FeatureToggleMenuButtonProps {
            disabled: false,
            name: props.label,
            working: false,
        });

        root.set_primary_icon(vec![Icon::parse(&Arc::from("preferences-system-symbolic"))]);

        let on_output: Callback<FeatureToggleMenuButtonOutput> = Rc::new(RefCell::new(None));
        let on_output_ref = on_output.clone();
        root.connect_output(move |_| {
            if let Some(ref callback) = *on_output_ref.borrow() {
                callback(FeatureToggleMenuButtonOutput::Click);
            }
        });

        Self { root, on_output }
    }

    pub fn on_click<F>(&self, callback: F)
    where
        F: Fn(FeatureToggleMenuButtonOutput) + 'static,
    {
        *self.on_output.borrow_mut() = Some(Box::new(callback));
    }

    pub fn set_visible(&self, visible: bool) {
        self.root.set_visible(visible);
    }

    pub fn widget(&self) -> gtk::Widget {
        self.root.widget()
    }
}
