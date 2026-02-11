//! ToggleButton widget renderer - icon toggle button with active state

use crate::renderer::ActionCallback;
use crate::widgets::icon::IconWidget;
use gtk::prelude::*;
use std::cell::RefCell;
use std::rc::Rc;
use waft_ipc::widget::{Action, ActionParams};

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

impl crate::reconcile::Reconcilable for ToggleButtonWidget {
    fn try_reconcile(
        &self,
        old_desc: &waft_ipc::Widget,
        new_desc: &waft_ipc::Widget,
    ) -> crate::reconcile::ReconcileOutcome {
        use crate::reconcile::ReconcileOutcome;
        match (old_desc, new_desc) {
            (
                waft_ipc::Widget::ToggleButton {
                    on_toggle: old_toggle,
                    ..
                },
                waft_ipc::Widget::ToggleButton {
                    icon,
                    active,
                    on_toggle: new_toggle,
                },
            ) => {
                if old_toggle != new_toggle {
                    return ReconcileOutcome::Recreate;
                }
                self.set_active(*active);
                self.set_icon(icon);
                ReconcileOutcome::Updated
            }
            _ => ReconcileOutcome::Recreate,
        }
    }
}

/// Render a ToggleButton widget from the IPC protocol using ToggleButtonWidget.
pub(crate) fn render_toggle_button(
    callback: &ActionCallback,
    icon: &str,
    active: bool,
    on_toggle: &Action,
    widget_id: &str,
) -> gtk::Widget {
    let toggle_button = ToggleButtonWidget::new(ToggleButtonProps {
        icon: icon.to_string(),
        active,
    });

    // Wire up the action callback for toggle clicks
    let cb = callback.clone();
    let wid = widget_id.to_string();
    let action = on_toggle.clone();
    toggle_button.root.connect_toggled(move |button| {
        let is_active = button.is_active();
        let mut a = action.clone();
        a.params = ActionParams::Value(if is_active { 1.0 } else { 0.0 });
        cb(wid.clone(), a);
    });

    toggle_button.widget()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::reconcile::Reconcilable;
    use crate::test_utils::init_gtk_for_tests;
    use crate::types::ActionParams;
    use std::cell::RefCell;
    use std::rc::Rc;

    fn dummy_action() -> Action {
        Action {
            id: "toggle".to_string(),
            params: ActionParams::None,
        }
    }

    #[test]
    #[ignore = "Requires GTK main thread - run with --test-threads=1"]
    fn test_render_toggle_button_inactive() {
        init_gtk_for_tests();
        let callback: ActionCallback = Rc::new(|_id, _action| {});

        let on_toggle = Action {
            id: "toggle_feature".to_string(),
            params: ActionParams::None,
        };

        let widget = render_toggle_button(&callback, "starred-symbolic", false, &on_toggle, "test_toggle");

        assert!(widget.is::<gtk::ToggleButton>());
        let toggle_button: gtk::ToggleButton = widget.downcast().unwrap();
        assert!(!toggle_button.is_active());
    }

    #[test]
    #[ignore = "Requires GTK main thread - run with --test-threads=1"]
    fn test_render_toggle_button_active() {
        init_gtk_for_tests();
        let callback: ActionCallback = Rc::new(|_id, _action| {});

        let on_toggle = Action {
            id: "toggle_feature".to_string(),
            params: ActionParams::None,
        };

        let widget = render_toggle_button(&callback, "starred-symbolic", true, &on_toggle, "test_toggle");

        let toggle_button: gtk::ToggleButton = widget.downcast().unwrap();
        assert!(toggle_button.is_active());
    }

    #[test]
    #[ignore = "Requires GTK main thread - run with --test-threads=1"]
    fn test_render_toggle_button_callback() {
        init_gtk_for_tests();

        let captured_actions: Rc<RefCell<Vec<(String, Action)>>> =
            Rc::new(RefCell::new(Vec::new()));
        let captured_actions_clone = captured_actions.clone();

        let callback: ActionCallback = Rc::new(move |widget_id, action| {
            captured_actions_clone
                .borrow_mut()
                .push((widget_id, action));
        });

        let on_toggle = Action {
            id: "toggle_feature".to_string(),
            params: ActionParams::None,
        };

        let widget = render_toggle_button(&callback, "starred-symbolic", false, &on_toggle, "test_toggle");

        let toggle_button: gtk::ToggleButton = widget.downcast().unwrap();

        // Simulate toggle
        toggle_button.set_active(true);

        // Verify callback was invoked
        let actions = captured_actions.borrow();
        assert_eq!(actions.len(), 1);
        assert_eq!(actions[0].0, "test_toggle");
        assert_eq!(actions[0].1.id, "toggle_feature");
        assert_eq!(actions[0].1.params, ActionParams::Value(1.0));
    }

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

    #[test]
    #[ignore = "Requires GTK main thread - run with --test-threads=1"]
    fn test_toggle_button_reconcile_updates() {
        init_gtk_for_tests();

        let old_widget = waft_ipc::Widget::ToggleButton {
            icon: "starred-symbolic".to_string(),
            active: false,
            on_toggle: dummy_action(),
        };

        let new_widget = waft_ipc::Widget::ToggleButton {
            icon: "emblem-favorite-symbolic".to_string(),
            active: true,
            on_toggle: dummy_action(),
        };

        let toggle_button = ToggleButtonWidget::new(ToggleButtonProps {
            icon: "starred-symbolic".to_string(),
            active: false,
        });

        let outcome = toggle_button.try_reconcile(&old_widget, &new_widget);
        assert_eq!(
            outcome,
            crate::reconcile::ReconcileOutcome::Updated,
            "Changing icon/active should update in-place"
        );
        assert!(toggle_button.root.is_active());
    }

    #[test]
    #[ignore = "Requires GTK main thread - run with --test-threads=1"]
    fn test_toggle_button_reconcile_recreate_on_action_change() {
        init_gtk_for_tests();

        let old_widget = waft_ipc::Widget::ToggleButton {
            icon: "starred-symbolic".to_string(),
            active: false,
            on_toggle: Action {
                id: "old_action".to_string(),
                params: ActionParams::None,
            },
        };

        let new_widget = waft_ipc::Widget::ToggleButton {
            icon: "starred-symbolic".to_string(),
            active: false,
            on_toggle: Action {
                id: "new_action".to_string(),
                params: ActionParams::None,
            },
        };

        let toggle_button = ToggleButtonWidget::new(ToggleButtonProps {
            icon: "starred-symbolic".to_string(),
            active: false,
        });

        let outcome = toggle_button.try_reconcile(&old_widget, &new_widget);
        assert_eq!(
            outcome,
            crate::reconcile::ReconcileOutcome::Recreate,
            "Changing action should recreate"
        );
    }
}
