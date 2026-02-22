//! ToggleButton widget - icon toggle button with active state

use crate::icons::IconWidget;
use gtk::prelude::*;
use std::cell::RefCell;
use std::rc::Rc;

/// Properties for initializing a toggle button.
#[derive(Debug, Clone)]
pub struct ToggleButtonProps {
    pub icon: String,
    pub active: bool,
}

/// Pure GTK4 toggle button widget with icon.
#[derive(Clone)]
pub struct ToggleButtonWidget {
    pub root: gtk::ToggleButton,
    icon_widget: IconWidget,
    active: Rc<RefCell<bool>>,
}

impl ToggleButtonWidget {
    /// Create a new toggle button widget.
    pub fn new(props: ToggleButtonProps) -> Self {
        let root = gtk::ToggleButton::builder()
            .active(props.active)
            .css_classes(["toggle-button"])
            .build();

        let icon_widget = IconWidget::from_name(&props.icon, 24);
        root.set_child(Some(icon_widget.widget()));

        let active = Rc::new(RefCell::new(props.active));

        Self {
            root,
            icon_widget,
            active,
        }
    }

    /// Set the active state.
    pub fn set_active(&self, active: bool) {
        *self.active.borrow_mut() = active;
        self.root.set_active(active);
    }

    /// Set the icon.
    pub fn set_icon(&self, icon: &str) {
        self.icon_widget.set_icon(icon);
    }

    /// Get a reference to the root widget.
    pub fn widget(&self) -> gtk::Widget {
        self.root.clone().upcast::<gtk::Widget>()
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

        assert!(!toggle_button.root.is_active());
        toggle_button.set_active(true);
        assert!(toggle_button.root.is_active());
        toggle_button.set_active(false);
        assert!(!toggle_button.root.is_active());
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
