//! Pure GTK4 Feature Toggle widget.
//!
//! A unified toggle button that can be simple or expandable.
//! When expandable=false, only shows the main toggle button.
//! When expandable=true, shows both main button and expand button with menu support.

use std::cell::RefCell;
use std::rc::Rc;

use gtk::prelude::*;
use uuid::Uuid;

use crate::widgets::menu_chevron::{MenuChevronProps, MenuChevronWidget};
use waft_core::Callback;
use crate::widgets::icon::IconWidget;
use waft_core::menu_state::{MenuOp, MenuStore};

use crate::renderer::ActionCallback;
use crate::menu_state::menu_id_for_widget;
use waft_ipc::widget::Action;

// Note: render_feature_toggle and related types below are pub(crate) to avoid
// leaking renderer-internal types through plugin-api's glob re-export.

/// Properties for initializing a feature toggle.
#[derive(Debug, Clone)]
pub struct FeatureToggleProps {
    pub active: bool,
    pub busy: bool,
    pub details: Option<String>,
    pub expandable: bool,
    pub icon: String,
    pub title: String,
    /// Optional deterministic menu ID. When provided, the toggle uses this
    /// instead of generating a random UUID. Callers should use
    /// `menu_id_for_widget(widget_id)` to produce a stable ID that
    /// matches any external content revealer.
    pub menu_id: Option<String>,
}

/// Output events from the feature toggle.
#[derive(Debug, Clone)]
pub enum FeatureToggleOutput {
    Activate,
    Deactivate,
}

/// Pure GTK4 feature toggle widget with optional expandable menu support.
#[derive(Clone)]
pub struct FeatureToggleWidget {
    pub root: gtk::Box,
    expand_revealer: gtk::Revealer,
    icon_widget: IconWidget,
    title_label: gtk::Label,
    details_label: gtk::Label,
    details_revealer: gtk::Revealer,
    active: Rc<RefCell<bool>>,
    busy: Rc<RefCell<bool>>,
    expandable: Rc<RefCell<bool>>,
    expanded: Rc<RefCell<bool>>,
    on_output: Callback<FeatureToggleOutput>,
    on_expand: Callback<bool>,
    pub menu_id: Option<String>,
}

impl FeatureToggleWidget {
    /// Create a new feature toggle widget.
    ///
    /// If menu_store is provided, the widget can be made expandable.
    /// The expand button visibility is controlled by the "expandable" CSS class.
    pub fn new(props: FeatureToggleProps, menu_store: Option<Rc<MenuStore>>) -> Self {
        // Use provided deterministic menu ID, or fall back to random UUID
        let menu_id = menu_store.as_ref().map(|_| {
            props.menu_id.clone().unwrap_or_else(|| Uuid::new_v4().to_string())
        });

        // Root container: horizontal box containing main button + expand button
        let root = gtk::Box::builder()
            .orientation(gtk::Orientation::Horizontal)
            .spacing(0)
            .hexpand(true)
            .css_classes(["feature-toggle"])
            .build();

        // Main button (toggle on/off)
        let main_button = gtk::Button::builder()
            .hexpand(true)
            .css_classes(["toggle-main"])
            .build();

        let main_content = gtk::Box::builder()
            .orientation(gtk::Orientation::Horizontal)
            .spacing(12)
            .valign(gtk::Align::Center)
            .build();

        let icon_widget = IconWidget::from_name(&props.icon, 24);
        icon_widget.widget().set_height_request(24);

        let text_content = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .valign(gtk::Align::Center)
            .spacing(2)
            .css_classes(["text-content"])
            .build();

        let title_label = gtk::Label::builder()
            .label(&props.title)
            .css_classes(["heading", "title"])
            .xalign(0.0)
            .build();

        let details_revealer = gtk::Revealer::builder()
            .transition_type(gtk::RevealerTransitionType::SlideDown)
            .reveal_child(props.details.is_some())
            .build();

        let details_label = gtk::Label::builder()
            .label(props.details.as_deref().unwrap_or(""))
            .css_classes(["dim-label", "caption"])
            .xalign(0.0)
            .build();

        details_revealer.set_child(Some(&details_label));

        text_content.append(&title_label);
        text_content.append(&details_revealer);

        main_content.append(icon_widget.widget());
        main_content.append(&text_content);

        main_button.set_child(Some(&main_content));

        // Expand button (with menu chevron)
        let menu_chevron = MenuChevronWidget::new(MenuChevronProps { expanded: false });
        let expand_button = gtk::Button::builder()
            .css_classes(["toggle-expand"])
            .build();
        expand_button.set_child(menu_chevron.widget());

        // Wrap expand button in revealer for smooth slide-left transition
        let expand_revealer = gtk::Revealer::builder()
            .transition_type(gtk::RevealerTransitionType::SlideLeft)
            .transition_duration(200) // 200ms transition
            .reveal_child(props.expandable)
            .build();
        expand_revealer.set_child(Some(&expand_button));

        // Add main button and revealer to root
        root.append(&main_button);
        root.append(&expand_revealer);

        let active = Rc::new(RefCell::new(props.active));
        let busy = Rc::new(RefCell::new(props.busy));
        let expandable = Rc::new(RefCell::new(props.expandable));
        let expanded = Rc::new(RefCell::new(false));
        let on_output: Callback<FeatureToggleOutput> = Rc::new(RefCell::new(None));
        let on_expand: Callback<bool> = Rc::new(RefCell::new(None));

        // Update CSS classes based on initial state
        Self::update_css_classes(&root, props.active, props.busy, props.expandable, false);

        // Connect main button click handler
        let active_ref = active.clone();
        let on_output_ref = on_output.clone();
        main_button.connect_clicked(move |_| {
            let is_active = *active_ref.borrow();
            if let Some(ref callback) = *on_output_ref.borrow() {
                if is_active {
                    callback(FeatureToggleOutput::Deactivate);
                } else {
                    callback(FeatureToggleOutput::Activate);
                }
            }
        });

        // Connect expand button click handler (if menu_store provided)
        if let Some(ref store) = menu_store {
            let menu_store_clone = store.clone();
            let menu_id_clone = menu_id.clone().unwrap();
            expand_button.connect_clicked(move |_| {
                // Always emit OpenMenu - MenuStore will handle toggle logic
                menu_store_clone.emit(MenuOp::OpenMenu(menu_id_clone.clone()));
            });

            // Subscribe to menu store updates
            let root_clone = root.clone();
            let menu_chevron_clone = menu_chevron.clone();
            let expanded_clone = expanded.clone();
            let active_clone = active.clone();
            let busy_clone = busy.clone();
            let expandable_clone = expandable.clone();
            let menu_store_clone = store.clone();
            let menu_id_clone = menu_id.clone().unwrap();
            let on_expand_clone = on_expand.clone();
            store.subscribe(move || {
                let state = menu_store_clone.get_state();
                let should_be_open = state.active_menu_id.as_ref() == Some(&menu_id_clone);

                *expanded_clone.borrow_mut() = should_be_open;
                menu_chevron_clone.set_expanded(should_be_open);
                Self::update_css_classes(
                    &root_clone,
                    *active_clone.borrow(),
                    *busy_clone.borrow(),
                    *expandable_clone.borrow(),
                    should_be_open,
                );

                // Notify plugin of expand state change
                if let Some(ref callback) = *on_expand_clone.borrow() {
                    callback(should_be_open);
                }
            });

            // Sync initial state
            {
                let state = store.get_state();
                let should_be_open =
                    state.active_menu_id.as_ref() == Some(menu_id.as_ref().unwrap());
                *expanded.borrow_mut() = should_be_open;
                menu_chevron.set_expanded(should_be_open);
                Self::update_css_classes(
                    &root,
                    *active.borrow(),
                    *busy.borrow(),
                    props.expandable,
                    should_be_open,
                );
            }
        }

        Self {
            root,
            expand_revealer,
            icon_widget,
            title_label,
            details_label,
            details_revealer,
            active,
            busy,
            expandable,
            expanded,
            on_output,
            on_expand,
            menu_id,
        }
    }

    /// Set the callback for expand state changes.
    pub fn set_expand_callback<F>(&self, callback: F)
    where
        F: Fn(bool) + 'static,
    {
        *self.on_expand.borrow_mut() = Some(Box::new(callback));
    }

    /// Set the callback for output events.
    pub fn connect_output<F>(&self, callback: F)
    where
        F: Fn(FeatureToggleOutput) + 'static,
    {
        *self.on_output.borrow_mut() = Some(Box::new(callback));
    }

    /// Update the active state.
    pub fn set_active(&self, active: bool) {
        *self.active.borrow_mut() = active;
        Self::update_css_classes(
            &self.root,
            active,
            *self.busy.borrow(),
            *self.expandable.borrow(),
            *self.expanded.borrow(),
        );
    }

    /// Update the busy state.
    pub fn set_busy(&self, busy: bool) {
        *self.busy.borrow_mut() = busy;
        Self::update_css_classes(
            &self.root,
            *self.active.borrow(),
            busy,
            *self.expandable.borrow(),
            *self.expanded.borrow(),
        );
    }

    /// Update the expandable state.
    /// When false, the expand button slides out (hidden).
    /// When true, the expand button slides in (visible).
    pub fn set_expandable(&self, expandable: bool) {
        *self.expandable.borrow_mut() = expandable;
        self.expand_revealer.set_reveal_child(expandable);
        Self::update_css_classes(
            &self.root,
            *self.active.borrow(),
            *self.busy.borrow(),
            expandable,
            *self.expanded.borrow(),
        );
    }

    /// Update the details text.
    pub fn set_details(&self, details: Option<String>) {
        self.details_revealer.set_reveal_child(details.is_some());
        self.details_label
            .set_label(details.as_deref().unwrap_or(""));
    }

    /// Update the icon.
    pub fn set_icon(&self, icon: &str) {
        self.icon_widget.set_icon(icon);
    }

    /// Update the title text.
    pub fn set_title(&self, title: &str) {
        self.title_label.set_label(title);
    }

    /// Get a reference to the root widget.
    pub fn widget(&self) -> gtk::Widget {
        self.root.clone().upcast::<gtk::Widget>()
    }

    fn update_css_classes(
        container: &gtk::Box,
        active: bool,
        busy: bool,
        expandable: bool,
        expanded: bool,
    ) {
        crate::css::apply_state_classes(
            container,
            Some("feature-toggle"),
            &[
                ("active", active),
                ("busy", busy),
                ("expandable", expandable),
                ("expanded", expanded),
            ],
        );
    }
}

impl crate::reconcile::Reconcilable for FeatureToggleWidget {
    fn try_reconcile(
        &self,
        old_desc: &waft_ipc::Widget,
        new_desc: &waft_ipc::Widget,
    ) -> crate::reconcile::ReconcileOutcome {
        use crate::reconcile::ReconcileOutcome;
        match (old_desc, new_desc) {
            (
                waft_ipc::Widget::FeatureToggle {
                    on_toggle: old_toggle,
                    ..
                },
                waft_ipc::Widget::FeatureToggle {
                    title,
                    icon,
                    details,
                    active,
                    busy,
                    expandable,
                    on_toggle: new_toggle,
                    ..
                },
            ) => {
                if old_toggle != new_toggle {
                    return ReconcileOutcome::Recreate;
                }
                self.set_active(*active);
                self.set_busy(*busy);
                self.set_details(details.clone());
                self.set_icon(icon);
                self.set_title(title);
                self.set_expandable(*expandable);
                ReconcileOutcome::Updated
            }
            _ => ReconcileOutcome::Recreate,
        }
    }
}

/// Render a FeatureToggle widget from the IPC protocol using FeatureToggleWidget.
///
/// This bridges the daemon widget protocol to the stateful FeatureToggleWidget,
/// ensuring daemon plugins and cdylib plugins use the same rendering.
#[allow(clippy::too_many_arguments)]
pub(crate) fn render_feature_toggle(
    _renderer: &crate::renderer::WidgetRenderer,
    callback: &ActionCallback,
    menu_store: &Rc<MenuStore>,
    title: &str,
    icon: &str,
    details: &Option<String>,
    active: bool,
    busy: bool,
    expandable: bool,
    _expanded_content: &Option<Box<crate::types::Widget>>,
    on_toggle: &Action,
    widget_id: &str,
) -> gtk::Widget {
    let toggle = FeatureToggleWidget::new(
        FeatureToggleProps {
            title: title.to_string(),
            icon: icon.to_string(),
            details: details.clone(),
            active,
            busy,
            expandable,
            menu_id: Some(menu_id_for_widget(widget_id)),
        },
        Some(menu_store.clone()),
    );

    // Wire up the action callback for toggle clicks
    let cb = callback.clone();
    let wid = widget_id.to_string();
    let action = on_toggle.clone();
    toggle.connect_output(move |output| {
        use waft_ipc::widget::ActionParams;
        let mut a = action.clone();
        a.params = match output {
            FeatureToggleOutput::Activate => ActionParams::Value(1.0),
            FeatureToggleOutput::Deactivate => ActionParams::Value(0.0),
        };
        cb(wid.clone(), a);
    });

    toggle.widget()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::reconcile::Reconcilable;
    use crate::test_utils::init_gtk_for_tests;
    use crate::types::ActionParams;
    use std::cell::RefCell;
    use waft_core::menu_state::create_menu_store;

    fn dummy_action() -> Action {
        Action {
            id: "toggle".to_string(),
            params: ActionParams::None,
        }
    }

    #[test]
    #[ignore = "Requires GTK main thread - run with --test-threads=1"]
    fn test_render_feature_toggle_inactive() {
        init_gtk_for_tests();
        let menu_store = Rc::new(create_menu_store());
        let callback: ActionCallback = Rc::new(|_id, _action| {});
        let renderer = crate::renderer::WidgetRenderer::new(menu_store.clone(), callback.clone());

        let on_toggle = Action {
            id: "toggle_bluetooth".to_string(),
            params: ActionParams::None,
        };

        let widget = render_feature_toggle(
            &renderer,
            &callback,
            &menu_store,
            "Bluetooth",
            "bluetooth-symbolic",
            &None,
            false,
            false,
            false,
            &None,
            &on_toggle,
            "bluetooth",
        );

        assert!(widget.is::<gtk::Box>());
        let main_box: gtk::Box = widget.downcast().unwrap();
        assert!(main_box.has_css_class("feature-toggle"));
        assert!(!main_box.has_css_class("active"));
        assert!(!main_box.has_css_class("busy"));
    }

    #[test]
    #[ignore = "Requires GTK main thread - run with --test-threads=1"]
    fn test_render_feature_toggle_active() {
        init_gtk_for_tests();
        let menu_store = Rc::new(create_menu_store());
        let callback: ActionCallback = Rc::new(|_id, _action| {});
        let renderer = crate::renderer::WidgetRenderer::new(menu_store.clone(), callback.clone());

        let on_toggle = Action {
            id: "toggle_wifi".to_string(),
            params: ActionParams::None,
        };

        let widget = render_feature_toggle(
            &renderer,
            &callback,
            &menu_store,
            "Wi-Fi",
            "network-wireless-symbolic",
            &None,
            true,
            false,
            false,
            &None,
            &on_toggle,
            "wifi",
        );

        let main_box: gtk::Box = widget.downcast().unwrap();
        assert!(main_box.has_css_class("active"));
    }

    #[test]
    #[ignore = "Requires GTK main thread - run with --test-threads=1"]
    fn test_render_feature_toggle_busy() {
        init_gtk_for_tests();
        let menu_store = Rc::new(create_menu_store());
        let callback: ActionCallback = Rc::new(|_id, _action| {});
        let renderer = crate::renderer::WidgetRenderer::new(menu_store.clone(), callback.clone());

        let on_toggle = Action {
            id: "toggle_feature".to_string(),
            params: ActionParams::None,
        };

        let widget = render_feature_toggle(
            &renderer,
            &callback,
            &menu_store,
            "Loading",
            "emblem-synchronizing-symbolic",
            &None,
            false,
            true,
            false,
            &None,
            &on_toggle,
            "loading_feature",
        );

        let main_box: gtk::Box = widget.downcast().unwrap();
        assert!(main_box.has_css_class("busy"));
    }

    #[test]
    #[ignore = "Requires GTK main thread - run with --test-threads=1"]
    fn test_render_feature_toggle_with_details() {
        init_gtk_for_tests();
        let menu_store = Rc::new(create_menu_store());
        let callback: ActionCallback = Rc::new(|_id, _action| {});
        let renderer = crate::renderer::WidgetRenderer::new(menu_store.clone(), callback.clone());

        let on_toggle = Action {
            id: "toggle_bt".to_string(),
            params: ActionParams::None,
        };

        let widget = render_feature_toggle(
            &renderer,
            &callback,
            &menu_store,
            "Bluetooth",
            "bluetooth-active-symbolic",
            &Some("Connected to 2 devices".to_string()),
            true,
            false,
            false,
            &None,
            &on_toggle,
            "bt_with_details",
        );

        assert!(widget.is::<gtk::Box>());
    }

    #[test]
    #[ignore = "Requires GTK main thread - run with --test-threads=1"]
    fn test_render_feature_toggle_expandable() {
        init_gtk_for_tests();
        let menu_store = Rc::new(create_menu_store());
        let callback: ActionCallback = Rc::new(|_id, _action| {});
        let renderer = crate::renderer::WidgetRenderer::new(menu_store.clone(), callback.clone());

        let on_toggle = Action {
            id: "toggle".to_string(),
            params: ActionParams::None,
        };

        let widget = render_feature_toggle(
            &renderer,
            &callback,
            &menu_store,
            "Settings",
            "preferences-system-symbolic",
            &None,
            false,
            false,
            true,
            &None,
            &on_toggle,
            "expandable_feature",
        );

        let main_box: gtk::Box = widget.downcast().unwrap();
        assert!(main_box.has_css_class("expandable"));
        assert!(!main_box.has_css_class("expanded")); // Not expanded by default
    }

    #[test]
    #[ignore = "Requires GTK main thread - run with --test-threads=1"]
    fn test_render_feature_toggle_active_expandable() {
        init_gtk_for_tests();
        let menu_store = Rc::new(create_menu_store());
        let callback: ActionCallback = Rc::new(|_id, _action| {});
        let renderer = crate::renderer::WidgetRenderer::new(menu_store.clone(), callback.clone());

        let on_toggle = Action {
            id: "toggle".to_string(),
            params: ActionParams::None,
        };

        let widget = render_feature_toggle(
            &renderer,
            &callback,
            &menu_store,
            "Bluetooth",
            "bluetooth-active-symbolic",
            &Some("Connected".to_string()),
            true,
            false,
            true,
            &None,
            &on_toggle,
            "active_expandable",
        );

        let main_box: gtk::Box = widget.downcast().unwrap();
        assert!(main_box.has_css_class("feature-toggle"));
        assert!(main_box.has_css_class("active"));
        assert!(main_box.has_css_class("expandable"));
    }

    #[test]
    #[ignore = "Requires GTK main thread - run with --test-threads=1"]
    fn test_render_feature_toggle_callback() {
        init_gtk_for_tests();
        let menu_store = Rc::new(create_menu_store());

        let captured_actions: Rc<RefCell<Vec<(String, Action)>>> =
            Rc::new(RefCell::new(Vec::new()));
        let captured_actions_clone = captured_actions.clone();

        let callback: ActionCallback = Rc::new(move |widget_id, action| {
            captured_actions_clone
                .borrow_mut()
                .push((widget_id, action));
        });

        let renderer = crate::renderer::WidgetRenderer::new(menu_store.clone(), callback.clone());

        let on_toggle = Action {
            id: "toggle_feature".to_string(),
            params: ActionParams::None,
        };

        let widget = render_feature_toggle(
            &renderer,
            &callback,
            &menu_store,
            "Test Feature",
            "dialog-information-symbolic",
            &None,
            false,
            false,
            false,
            &None,
            &on_toggle,
            "test_feature",
        );

        let main_box: gtk::Box = widget.downcast().unwrap();
        let main_button = main_box.first_child().unwrap();
        let main_button: gtk::Button = main_button.downcast().unwrap();

        // Simulate button click
        main_button.emit_clicked();

        // Verify callback was invoked
        let actions = captured_actions.borrow();
        assert_eq!(actions.len(), 1);
        assert_eq!(actions[0].0, "test_feature");
        assert_eq!(actions[0].1.id, "toggle_feature");
    }

    #[test]
    #[ignore = "Requires GTK main thread - run with --test-threads=1"]
    fn test_render_feature_toggle_all_states() {
        init_gtk_for_tests();
        let menu_store = Rc::new(create_menu_store());
        let callback: ActionCallback = Rc::new(|_id, _action| {});
        let renderer = crate::renderer::WidgetRenderer::new(menu_store.clone(), callback.clone());

        let on_toggle = Action {
            id: "toggle".to_string(),
            params: ActionParams::None,
        };

        // Test all state combinations
        let widget = render_feature_toggle(
            &renderer,
            &callback,
            &menu_store,
            "Full Featured",
            "starred-symbolic",
            &Some("All features enabled".to_string()),
            true,
            true,
            true,
            &None,
            &on_toggle,
            "full_featured",
        );

        let main_box: gtk::Box = widget.downcast().unwrap();
        assert!(main_box.has_css_class("feature-toggle"));
        assert!(main_box.has_css_class("active"));
        assert!(main_box.has_css_class("busy"));
        assert!(main_box.has_css_class("expandable"));
    }

    #[test]
    #[ignore = "Requires GTK main thread - run with --test-threads=1"]
    fn test_feature_toggle_expanded_content_change_updates_in_place() {
        init_gtk_for_tests();
        let menu_store = Rc::new(create_menu_store());

        let old_widget = waft_ipc::Widget::FeatureToggle {
            title: "Bluetooth".to_string(),
            icon: "bluetooth-symbolic".to_string(),
            details: Some("1 connected".to_string()),
            active: true,
            busy: false,
            expandable: true,
            expanded_content: Some(Box::new(waft_ipc::Widget::Col {
                spacing: 4,
                css_classes: vec![],
                children: vec![
                    waft_ipc::Node::keyed(
                        "device1",
                        waft_ipc::Widget::Label {
                            text: "Headphones".to_string(),
                            css_classes: vec![],
                        },
                    ),
                ],
            })),
            on_toggle: dummy_action(),
        };

        let new_widget = waft_ipc::Widget::FeatureToggle {
            title: "Bluetooth".to_string(),
            icon: "bluetooth-symbolic".to_string(),
            details: Some("2 connected".to_string()),
            active: true,
            busy: false,
            expandable: true,
            expanded_content: Some(Box::new(waft_ipc::Widget::Col {
                spacing: 4,
                css_classes: vec![],
                children: vec![
                    waft_ipc::Node::keyed(
                        "device1",
                        waft_ipc::Widget::Label {
                            text: "Headphones".to_string(),
                            css_classes: vec![],
                        },
                    ),
                    waft_ipc::Node::keyed(
                        "device2",
                        waft_ipc::Widget::Label {
                            text: "Speaker".to_string(),
                            css_classes: vec![],
                        },
                    ),
                ],
            })),
            on_toggle: dummy_action(),
        };

        let toggle = FeatureToggleWidget::new(
            FeatureToggleProps {
                title: "Bluetooth".to_string(),
                icon: "bluetooth-symbolic".to_string(),
                details: Some("1 connected".to_string()),
                active: true,
                busy: false,
                expandable: true,
                menu_id: None,
            },
            Some(menu_store),
        );

        let outcome = toggle.try_reconcile(&old_widget, &new_widget);
        assert_eq!(
            outcome,
            crate::reconcile::ReconcileOutcome::Updated,
            "Changing expanded_content should update in-place (menu swap handled by reconciler)"
        );
    }

    #[test]
    #[ignore = "Requires GTK main thread - run with --test-threads=1"]
    fn test_feature_toggle_expanded_content_none_to_some_updates_in_place() {
        init_gtk_for_tests();
        let menu_store = Rc::new(create_menu_store());

        let old_widget = waft_ipc::Widget::FeatureToggle {
            title: "Bluetooth".to_string(),
            icon: "bluetooth-symbolic".to_string(),
            details: None,
            active: true,
            busy: false,
            expandable: true,
            expanded_content: None,
            on_toggle: dummy_action(),
        };

        let new_widget = waft_ipc::Widget::FeatureToggle {
            title: "Bluetooth".to_string(),
            icon: "bluetooth-symbolic".to_string(),
            details: Some("1 connected".to_string()),
            active: true,
            busy: false,
            expandable: true,
            expanded_content: Some(Box::new(waft_ipc::Widget::Label {
                text: "Headphones".to_string(),
                css_classes: vec![],
            })),
            on_toggle: dummy_action(),
        };

        let toggle = FeatureToggleWidget::new(
            FeatureToggleProps {
                title: "Bluetooth".to_string(),
                icon: "bluetooth-symbolic".to_string(),
                details: None,
                active: true,
                busy: false,
                expandable: true,
                menu_id: None,
            },
            Some(menu_store),
        );

        let outcome = toggle.try_reconcile(&old_widget, &new_widget);
        assert_eq!(
            outcome,
            crate::reconcile::ReconcileOutcome::Updated,
            "None→Some handled by reconciler, widget level returns Updated"
        );
    }

    #[test]
    #[ignore = "Requires GTK main thread - run with --test-threads=1"]
    fn test_feature_toggle_expanded_content_some_to_none_updates_in_place() {
        init_gtk_for_tests();
        let menu_store = Rc::new(create_menu_store());

        let old_widget = waft_ipc::Widget::FeatureToggle {
            title: "Bluetooth".to_string(),
            icon: "bluetooth-symbolic".to_string(),
            details: Some("1 connected".to_string()),
            active: true,
            busy: false,
            expandable: true,
            expanded_content: Some(Box::new(waft_ipc::Widget::Label {
                text: "Headphones".to_string(),
                css_classes: vec![],
            })),
            on_toggle: dummy_action(),
        };

        let new_widget = waft_ipc::Widget::FeatureToggle {
            title: "Bluetooth".to_string(),
            icon: "bluetooth-symbolic".to_string(),
            details: None,
            active: true,
            busy: false,
            expandable: true,
            expanded_content: None,
            on_toggle: dummy_action(),
        };

        let toggle = FeatureToggleWidget::new(
            FeatureToggleProps {
                title: "Bluetooth".to_string(),
                icon: "bluetooth-symbolic".to_string(),
                details: Some("1 connected".to_string()),
                active: true,
                busy: false,
                expandable: true,
                menu_id: None,
            },
            Some(menu_store),
        );

        let outcome = toggle.try_reconcile(&old_widget, &new_widget);
        assert_eq!(
            outcome,
            crate::reconcile::ReconcileOutcome::Updated,
            "Some→None handled by reconciler, widget level returns Updated"
        );
    }

    #[test]
    #[ignore = "Requires GTK main thread - run with --test-threads=1"]
    fn test_feature_toggle_expanded_content_unchanged_updates_in_place() {
        init_gtk_for_tests();
        let menu_store = Rc::new(create_menu_store());

        let old_widget = waft_ipc::Widget::FeatureToggle {
            title: "Bluetooth".to_string(),
            icon: "bluetooth-symbolic".to_string(),
            details: Some("1 connected".to_string()),
            active: false,
            busy: false,
            expandable: true,
            expanded_content: Some(Box::new(waft_ipc::Widget::Label {
                text: "Headphones".to_string(),
                css_classes: vec![],
            })),
            on_toggle: dummy_action(),
        };

        let new_widget = waft_ipc::Widget::FeatureToggle {
            title: "Bluetooth".to_string(),
            icon: "bluetooth-symbolic".to_string(),
            details: Some("1 connected".to_string()),
            active: true,
            busy: false,
            expandable: true,
            expanded_content: Some(Box::new(waft_ipc::Widget::Label {
                text: "Headphones".to_string(),
                css_classes: vec![],
            })),
            on_toggle: dummy_action(),
        };

        let toggle = FeatureToggleWidget::new(
            FeatureToggleProps {
                title: "Bluetooth".to_string(),
                icon: "bluetooth-symbolic".to_string(),
                details: Some("1 connected".to_string()),
                active: false,
                busy: false,
                expandable: true,
                menu_id: None,
            },
            Some(menu_store),
        );

        let outcome = toggle.try_reconcile(&old_widget, &new_widget);
        assert_eq!(
            outcome,
            crate::reconcile::ReconcileOutcome::Updated,
            "When expanded_content is unchanged, should update in-place"
        );
    }
}
