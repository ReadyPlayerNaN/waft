//! ToggleButton widget - declarative icon toggle button with active state

use std::cell::RefCell;
use std::rc::Rc;

use crate::icons::Icon;
use crate::vdom::{Component, RenderCallback, RenderComponent, RenderFn, VIcon, VNode, VToggleButton};

/// Properties for the toggle button.
#[derive(Clone, PartialEq, Debug)]
pub struct ToggleButtonProps {
    pub icon: String,
    pub active: bool,
}

pub enum ToggleButtonOutput {}

pub struct ToggleButtonRender;

impl RenderFn for ToggleButtonRender {
    type Props = ToggleButtonProps;
    type Output = ToggleButtonOutput;

    fn render(props: &Self::Props, _emit: &RenderCallback<ToggleButtonOutput>) -> VNode {
        let icon = VIcon::new(vec![Icon::Themed(props.icon.clone())], 24);
        let toggle_button = VToggleButton::new(props.active, VNode::icon(icon))
            .css_class("toggle-button");
        VNode::toggle_button(toggle_button)
    }
}

/// Wrapper around RenderComponent<ToggleButtonRender> with state tracking.
#[derive(Clone)]
pub struct ToggleButtonWidget {
    inner: Rc<RenderComponent<ToggleButtonRender>>,
    props: Rc<RefCell<ToggleButtonProps>>,
}

impl ToggleButtonWidget {
    /// Create a new toggle button widget.
    pub fn new(props: ToggleButtonProps) -> Self {
        let inner = Rc::new(RenderComponent::<ToggleButtonRender>::build(&props));
        Self {
            inner,
            props: Rc::new(RefCell::new(props)),
        }
    }

    /// Set the active state.
    pub fn set_active(&self, active: bool) {
        let mut props = self.props.borrow_mut();
        props.active = active;
        self.inner.update(&*props);
    }

    /// Set the icon.
    pub fn set_icon(&self, icon: &str) {
        let mut props = self.props.borrow_mut();
        props.icon = icon.to_string();
        self.inner.update(&*props);
    }

    /// Get a reference to the root widget.
    pub fn widget(&self) -> gtk::Widget {
        self.inner.widget()
    }
}

impl crate::widget_base::WidgetBase for ToggleButtonWidget {
    fn widget(&self) -> gtk::Widget {
        self.widget()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_init::init_gtk_for_tests;

    #[test]
    #[ignore = "Requires GTK main thread - run with --test-threads=1"]
    fn test_toggle_button_widget_set_active() {
        init_gtk_for_tests();

        let toggle_button = ToggleButtonWidget::new(ToggleButtonProps {
            icon: "starred-symbolic".to_string(),
            active: false,
        });

        // Verify widget is created
        let _ = toggle_button.widget();
        toggle_button.set_active(true);
        toggle_button.set_active(false);
    }

    #[test]
    #[ignore = "Requires GTK main thread - run with --test-threads=1"]
    fn test_toggle_button_widget_set_icon() {
        init_gtk_for_tests();

        let toggle_button = ToggleButtonWidget::new(ToggleButtonProps {
            icon: "starred-symbolic".to_string(),
            active: false,
        });

        toggle_button.set_icon("emblem-favorite-symbolic");
        // Icon change is internal - just verify no crash
    }
}
