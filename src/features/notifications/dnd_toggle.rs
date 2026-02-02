//! Do Not Disturb toggle widget.

use std::cell::RefCell;
use std::rc::Rc;

use crate::ui::feature_toggle::{FeatureToggleOutput, FeatureToggleProps, FeatureToggleWidget};

/// Output events from the DnD toggle.
#[derive(Debug, Clone)]
pub enum DoNotDisturbToggleOutput {
    Activate,
    Deactivate,
}

/// Properties for initializing the DnD toggle.
pub struct DoNotDisturbToggleInit {
    pub active: bool,
    pub busy: bool,
}

/// Do Not Disturb toggle using pure GTK4 FeatureToggleWidget.
pub struct DoNotDisturbToggleWidget {
    toggle: FeatureToggleWidget,
    on_output: Rc<RefCell<Option<Box<dyn Fn(DoNotDisturbToggleOutput)>>>>,
}

impl DoNotDisturbToggleWidget {
    /// Create a new DnD toggle widget.
    pub fn new(init: DoNotDisturbToggleInit) -> Self {
        let toggle = FeatureToggleWidget::new(
            FeatureToggleProps {
                title: crate::i18n::t("dnd-title").into(),
                icon: "notifications-disabled-symbolic".into(),
                details: None,
                active: init.active,
                busy: init.busy,
                expandable: false,
            },
            None, // No menu support
        );

        let on_output: Rc<RefCell<Option<Box<dyn Fn(DoNotDisturbToggleOutput)>>>> =
            Rc::new(RefCell::new(None));

        // Connect toggle output to our output
        let on_output_ref = on_output.clone();
        toggle.connect_output(move |event| {
            if let Some(ref callback) = *on_output_ref.borrow() {
                match event {
                    FeatureToggleOutput::Activate => {
                        callback(DoNotDisturbToggleOutput::Activate);
                    }
                    FeatureToggleOutput::Deactivate => {
                        callback(DoNotDisturbToggleOutput::Deactivate);
                    }
                }
            }
        });

        Self { toggle, on_output }
    }

    /// Set the callback for output events.
    pub fn connect_output<F>(&self, callback: F)
    where
        F: Fn(DoNotDisturbToggleOutput) + 'static,
    {
        *self.on_output.borrow_mut() = Some(Box::new(callback));
    }

    /// Update the active state.
    pub fn set_active(&self, active: bool) {
        self.toggle.set_active(active);
        self.toggle.set_busy(false);
    }

    /// Get a reference to the root widget.
    pub fn widget(&self) -> gtk::Widget {
        self.toggle.widget()
    }
}
