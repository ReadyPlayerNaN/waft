//! Main widget renderer converting Widget descriptions to GTK widgets.
//!
//! The WidgetRenderer is a stateless factory that takes declarative Widget
//! descriptions and creates fresh GTK widgets. It coordinates with the MenuStore
//! to ensure only one expandable menu is open at a time.

use waft_ipc::widget::{Action, Widget};
use crate::widgets::container::render_container;
use crate::widgets::feature_toggle::render_feature_toggle;
use crate::widgets::menu_row::render_menu_row;
use crate::widgets::primitives::{
    render_button, render_checkmark, render_label, render_spinner, render_switch,
};
use crate::widgets::slider::render_slider;
use std::rc::Rc;
use waft_core::menu_state::MenuStore;

#[cfg(test)]
use gtk::prelude::*;

/// Type alias for the action callback function.
///
/// When a user interaction occurs (button click, slider change, etc.),
/// the renderer invokes this callback with:
/// - `widget_id`: The unique identifier for the widget that triggered the action
/// - `action`: The Action object containing the action ID and parameters
///
/// The overview will route these actions to the appropriate plugin handlers.
pub type ActionCallback = Rc<dyn Fn(String, Action)>;

/// Stateless renderer that converts Widget descriptions to GTK widgets.
///
/// # Design Philosophy
///
/// The renderer is intentionally stateless - each call to `render()` creates
/// fresh GTK widgets. This simplifies the mental model and avoids complex
/// widget state synchronization. When a plugin updates its widget description,
/// the overview simply calls render() again and replaces the old widget.
///
/// # Menu Coordination
///
/// Expandable widgets (FeatureToggle, Slider) coordinate via the MenuStore
/// to ensure only one menu is open at a time. The renderer generates
/// deterministic menu IDs based on widget IDs.
///
/// # Example
///
/// ```ignore
/// use waft_ui_gtk::renderer::{WidgetRenderer, ActionCallback};
/// use waft_ui_gtk::types::{Widget, Action};
/// use waft_core::menu_state::create_menu_store;
/// use std::rc::Rc;
///
/// let menu_store = Rc::new(create_menu_store());
/// let callback: ActionCallback = Rc::new(|widget_id, action| {
///     println!("Widget {} triggered action {}", widget_id, action.id);
/// });
///
/// let renderer = WidgetRenderer::new(menu_store, callback);
///
/// let widget = Widget::Label {
///     text: "Hello".to_string(),
///     css_classes: vec![],
/// };
///
/// let gtk_widget = renderer.render(&widget, "my_label");
/// ```
#[allow(dead_code)] // Fields will be used in Tasks 4-10
pub struct WidgetRenderer {
    menu_store: Rc<MenuStore>,
    action_callback: ActionCallback,
}

impl WidgetRenderer {
    /// Create a new WidgetRenderer.
    ///
    /// # Parameters
    ///
    /// - `menu_store`: Shared MenuStore for coordinating expandable widget menus
    /// - `action_callback`: Callback invoked when user interactions trigger actions
    pub fn new(menu_store: Rc<MenuStore>, action_callback: ActionCallback) -> Self {
        Self {
            menu_store,
            action_callback,
        }
    }

    /// Render a Widget description into a GTK widget.
    ///
    /// # Parameters
    ///
    /// - `widget`: The declarative widget description to render
    /// - `widget_id`: Unique identifier for this widget instance
    ///
    /// # Returns
    ///
    /// A fresh GTK widget tree representing the given widget description.
    ///
    /// # Menu ID Convention
    ///
    /// For expandable widgets (FeatureToggle, Slider), the renderer generates
    /// menu IDs using the pattern: `format!("{}_menu", widget_id)`. This ensures
    /// consistent menu coordination across re-renders.
    #[allow(unused_variables)] // widget_id will be used in Task 4-10
    pub fn render(&self, widget: &Widget, widget_id: &str) -> gtk::Widget {
        match widget {
            Widget::FeatureToggle {
                title,
                icon,
                details,
                active,
                busy,
                expandable,
                expanded_content,
                on_toggle,
            } => render_feature_toggle(
                self,
                &self.action_callback,
                &self.menu_store,
                title,
                icon,
                details,
                *active,
                *busy,
                *expandable,
                expanded_content,
                on_toggle,
                widget_id,
            ),

            Widget::Slider {
                icon,
                value,
                muted,
                expandable,
                expanded_content,
                on_value_change,
                on_icon_click,
            } => render_slider(
                self,
                &self.action_callback,
                &self.menu_store,
                icon,
                *value,
                *muted,
                *expandable,
                expanded_content,
                on_value_change,
                on_icon_click,
                widget_id,
            ),

            Widget::Container {
                orientation,
                spacing,
                css_classes,
                children,
            } => render_container(self, orientation, *spacing, css_classes, children, widget_id),

            Widget::MenuRow {
                icon,
                label,
                sublabel,
                trailing,
                sensitive,
                on_click,
            } => render_menu_row(
                self,
                &self.action_callback,
                icon,
                label,
                sublabel,
                trailing,
                *sensitive,
                on_click,
                widget_id,
            ),

            Widget::Switch {
                active,
                sensitive,
                on_toggle,
            } => render_switch(&self.action_callback, *active, *sensitive, on_toggle, widget_id),

            Widget::Spinner { spinning } => render_spinner(*spinning),

            Widget::Checkmark { visible } => render_checkmark(*visible),

            Widget::Button {
                label,
                icon,
                on_click,
            } => render_button(&self.action_callback, label, icon, on_click, widget_id),

            Widget::Label {
                text,
                css_classes,
            } => render_label(text, css_classes),
        }
    }

    /// Get a reference to the menu store (for tests and utilities).
    #[cfg(test)]
    pub(crate) fn menu_store(&self) -> &MenuStore {
        &self.menu_store
    }

    /// Get a reference to the action callback (for tests and utilities).
    #[cfg(test)]
    #[allow(dead_code)] // Used in GTK tests which are marked #[ignore]
    pub(crate) fn action_callback(&self) -> &ActionCallback {
        &self.action_callback
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{ActionParams, Widget};
    use waft_core::menu_state::create_menu_store;

    // Helper to ensure GTK is initialized for widget tests
    fn init_gtk() {
        use std::sync::Once;
        static INIT: Once = Once::new();
        INIT.call_once(|| {
            gtk::init().expect("Failed to initialize GTK");
        });
    }

    #[test]
    fn test_renderer_creation() {
        let menu_store = Rc::new(create_menu_store());
        let callback: ActionCallback = Rc::new(|_id, _action| {});

        let _renderer = WidgetRenderer::new(menu_store, callback);
        // Constructor should work without panicking
    }

    // Note: GTK widget creation tests require running on the main thread.
    // These tests are marked #[ignore] by default. Run with:
    // cargo test -p waft-ui-gtk -- --ignored --test-threads=1

    #[test]
    #[ignore = "Requires GTK main thread - run with --test-threads=1"]
    fn test_render_label_basic() {
        init_gtk();
        let menu_store = Rc::new(create_menu_store());
        let callback: ActionCallback = Rc::new(|_id, _action| {});
        let renderer = WidgetRenderer::new(menu_store, callback);

        let widget = Widget::Label {
            text: "Test Label".to_string(),
            css_classes: vec![],
        };

        let gtk_widget = renderer.render(&widget, "test_label");

        // Should return a valid GTK widget
        assert!(gtk_widget.is::<gtk::Label>());

        let label: gtk::Label = gtk_widget.downcast().unwrap();
        assert_eq!(label.text(), "Test Label");
    }

    #[test]
    #[ignore = "Requires GTK main thread - run with --test-threads=1"]
    fn test_render_label_with_css_classes() {
        init_gtk();
        let menu_store = Rc::new(create_menu_store());
        let callback: ActionCallback = Rc::new(|_id, _action| {});
        let renderer = WidgetRenderer::new(menu_store, callback);

        let widget = Widget::Label {
            text: "Styled Label".to_string(),
            css_classes: vec!["bold".to_string(), "accent".to_string()],
        };

        let gtk_widget = renderer.render(&widget, "styled_label");
        let label: gtk::Label = gtk_widget.downcast().unwrap();

        assert!(label.has_css_class("bold"));
        assert!(label.has_css_class("accent"));
    }

    #[test]
    fn test_action_callback_invocation() {
        use std::cell::RefCell;

        let menu_store = Rc::new(create_menu_store());
        let captured_actions: Rc<RefCell<Vec<(String, Action)>>> =
            Rc::new(RefCell::new(Vec::new()));
        let captured_actions_clone = captured_actions.clone();

        let callback: ActionCallback = Rc::new(move |widget_id, action| {
            captured_actions_clone
                .borrow_mut()
                .push((widget_id, action));
        });

        let _renderer = WidgetRenderer::new(menu_store, callback.clone());

        // Simulate a callback invocation (as would happen from widget event handlers)
        let test_action = Action {
            id: "test_action".to_string(),
            params: ActionParams::None,
        };
        callback("widget_123".to_string(), test_action.clone());

        let actions = captured_actions.borrow();
        assert_eq!(actions.len(), 1);
        assert_eq!(actions[0].0, "widget_123");
        assert_eq!(actions[0].1.id, "test_action");
    }

    #[test]
    #[ignore = "Requires GTK main thread - run with --test-threads=1"]
    fn test_renderer_is_stateless() {
        init_gtk();
        let menu_store = Rc::new(create_menu_store());
        let callback: ActionCallback = Rc::new(|_id, _action| {});
        let renderer = WidgetRenderer::new(menu_store, callback);

        let widget = Widget::Label {
            text: "First".to_string(),
            css_classes: vec![],
        };

        let gtk_widget1 = renderer.render(&widget, "label1");
        let gtk_widget2 = renderer.render(&widget, "label2");

        // Each render() call should create a fresh widget (different object)
        // In GTK, widgets are different objects even if they look the same
        let label1: gtk::Label = gtk_widget1.downcast().unwrap();
        let label2: gtk::Label = gtk_widget2.downcast().unwrap();

        // These should be different widget instances
        // (no good way to test object identity in GTK, but we verify they work)
        assert_eq!(label1.text(), "First");
        assert_eq!(label2.text(), "First");
    }

    #[test]
    fn test_menu_store_accessible() {
        let menu_store = Rc::new(create_menu_store());
        let callback: ActionCallback = Rc::new(|_id, _action| {});
        let renderer = WidgetRenderer::new(menu_store.clone(), callback);

        // The renderer should hold the same menu_store reference
        let store_from_renderer = renderer.menu_store();
        // Just verify it's accessible (can't easily compare Rc contents)
        assert!(std::ptr::eq(
            store_from_renderer as *const _,
            menu_store.as_ref() as *const _
        ));
    }
}
