//! Details widget renderer - collapsible details with summary and expandable content

use crate::menu_state::menu_id_for_widget;
use crate::reconcile::Reconcilable;
use crate::renderer::{ActionCallback, WidgetRenderer};
use crate::widgets::menu_chevron::{MenuChevronProps, MenuChevronWidget};
use gtk::prelude::*;
use std::rc::Rc;
use waft_core::menu_state::{MenuOp, MenuStore};
use waft_ipc::widget::{Action, ActionParams, Node};

/// Properties for initializing a details widget.
#[derive(Debug, Clone)]
pub struct DetailsProps {
    pub menu_id: String,
}

/// Pure GTK4 details widget with collapsible content.
///
/// Structure:
/// - Root: gtk::Box (Vertical)
///   - Summary row: gtk::Button containing summary widget + menu chevron
///   - Content revealer: gtk::Revealer containing content widget
#[derive(Clone)]
pub struct DetailsWidget {
    pub root: gtk::Box,
    summary_button: gtk::Button,
    content_revealer: gtk::Revealer,
    menu_chevron: MenuChevronWidget,
    pub menu_id: String,
}

impl DetailsWidget {
    /// Create a new details widget.
    ///
    /// The summary and content are rendered by the WidgetRenderer.
    pub fn new(
        props: DetailsProps,
        summary_gtk: gtk::Widget,
        content_gtk: gtk::Widget,
        css_classes: &[String],
        menu_store: Rc<MenuStore>,
    ) -> Self {
        // Root container: vertical box
        let root = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .spacing(0)
            .css_classes(["details-widget"])
            .build();

        // Apply custom CSS classes
        for css_class in css_classes {
            root.add_css_class(css_class);
        }

        // Summary button (clickable row with chevron)
        let summary_button = gtk::Button::builder()
            .css_classes(["details-summary"])
            .build();

        // Create horizontal container for summary content + chevron
        let summary_container = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        summary_container.set_hexpand(true);

        // Add the summary widget (left side, expands)
        let summary_box = gtk::Box::new(gtk::Orientation::Horizontal, 0);
        summary_box.set_hexpand(true);
        summary_box.append(&summary_gtk);
        summary_container.append(&summary_box);

        // Add the menu chevron (right side)
        let is_open = menu_store
            .get_state()
            .active_menu_id
            .as_ref()
            .map(|id| id == &props.menu_id)
            .unwrap_or(false);
        let menu_chevron = MenuChevronWidget::new(MenuChevronProps { expanded: is_open });
        summary_container.append(&menu_chevron.root);

        summary_button.set_child(Some(&summary_container));

        // Content revealer
        let content_revealer = gtk::Revealer::builder()
            .transition_type(gtk::RevealerTransitionType::SlideDown)
            .transition_duration(200)
            .reveal_child(is_open)
            .build();

        content_revealer.set_child(Some(&content_gtk));

        // Assemble the widget
        root.append(&summary_button);
        root.append(&content_revealer);

        // Connect summary button click handler
        let menu_store_clone = menu_store.clone();
        let menu_id_clone = props.menu_id.clone();
        summary_button.connect_clicked(move |_| {
            menu_store_clone.emit(MenuOp::OpenMenu(menu_id_clone.clone()));
        });

        // Subscribe to menu store updates for expand/collapse
        let content_revealer_clone = content_revealer.clone();
        let menu_chevron_clone = menu_chevron.clone();
        let menu_id_clone = props.menu_id.clone();
        let menu_store_sub = menu_store.clone();
        menu_store.subscribe(move || {
            let state = menu_store_sub.get_state();
            let should_be_open = state.active_menu_id.as_ref() == Some(&menu_id_clone);
            content_revealer_clone.set_reveal_child(should_be_open);
            menu_chevron_clone.set_expanded(should_be_open);
        });

        Self {
            root,
            summary_button,
            content_revealer,
            menu_chevron,
            menu_id: props.menu_id,
        }
    }

    /// Get a reference to the root widget.
    pub fn widget(&self) -> gtk::Widget {
        self.root.clone().upcast::<gtk::Widget>()
    }
}

impl crate::reconcile::Reconcilable for DetailsWidget {
    fn try_reconcile(
        &self,
        old_desc: &waft_ipc::Widget,
        new_desc: &waft_ipc::Widget,
    ) -> crate::reconcile::ReconcileOutcome {
        use crate::reconcile::ReconcileOutcome;
        match (old_desc, new_desc) {
            (
                waft_ipc::Widget::Details {
                    on_toggle: old_toggle,
                    css_classes: old_css,
                    ..
                },
                waft_ipc::Widget::Details {
                    on_toggle: new_toggle,
                    css_classes: new_css,
                    ..
                },
            ) => {
                // If action or CSS classes change, recreate
                if old_toggle != new_toggle || old_css != new_css {
                    return ReconcileOutcome::Recreate;
                }
                // Summary and content changes are handled by the reconciler
                ReconcileOutcome::Updated
            }
            _ => ReconcileOutcome::Recreate,
        }
    }
}

/// Render a Details widget from the IPC protocol using DetailsWidget.
///
/// This bridges the daemon widget protocol to the stateful DetailsWidget.
pub(crate) fn render_details(
    renderer: &WidgetRenderer,
    callback: &ActionCallback,
    menu_store: &Rc<MenuStore>,
    summary: &waft_ipc::Widget,
    content: &waft_ipc::Widget,
    css_classes: &[String],
    on_toggle: &Action,
    widget_id: &str,
) -> gtk::Widget {
    let menu_id = menu_id_for_widget(widget_id);

    // Render summary and content widgets
    let summary_id = format!("{}:summary", widget_id);
    let content_id = format!("{}:content", widget_id);
    let summary_gtk = renderer.render(summary, &summary_id);
    let content_gtk = renderer.render(content, &content_id);

    let details = DetailsWidget::new(
        DetailsProps { menu_id: menu_id.clone() },
        summary_gtk,
        content_gtk,
        css_classes,
        menu_store.clone(),
    );

    // Wire up the action callback for toggle events
    let cb = callback.clone();
    let wid = widget_id.to_string();
    let action = on_toggle.clone();
    let store_clone = menu_store.clone();
    let menu_id_clone = menu_id.clone();
    menu_store.subscribe(move || {
        let state = store_clone.get_state();
        let is_open = state.active_menu_id.as_ref() == Some(&menu_id_clone);
        let mut a = action.clone();
        a.params = ActionParams::Value(if is_open { 1.0 } else { 0.0 });
        cb(wid.clone(), a);
    });

    details.widget()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_utils::init_gtk_for_tests;
    use crate::types::{ActionParams, Widget};
    use waft_core::menu_state::create_menu_store;

    fn dummy_action() -> Action {
        Action {
            id: "toggle".to_string(),
            params: ActionParams::None,
        }
    }

    #[test]
    #[ignore = "Requires GTK main thread - run with --test-threads=1"]
    fn test_render_details_collapsed() {
        init_gtk_for_tests();
        let menu_store = Rc::new(create_menu_store());
        let callback: ActionCallback = Rc::new(|_id, _action| {});
        let renderer = WidgetRenderer::new(menu_store.clone(), callback.clone());

        let summary = Widget::Label {
            text: "Summary".to_string(),
            css_classes: vec![],
        };

        let content = Widget::Label {
            text: "Detailed content".to_string(),
            css_classes: vec![],
        };

        let on_toggle = Action {
            id: "toggle_details".to_string(),
            params: ActionParams::None,
        };

        let widget = render_details(
            &renderer,
            &callback,
            &menu_store,
            &summary,
            &content,
            &[],
            &on_toggle,
            "test_details",
        );

        assert!(widget.is::<gtk::Box>());
        let main_box: gtk::Box = widget.downcast().unwrap();
        assert_eq!(main_box.orientation(), gtk::Orientation::Vertical);
        assert!(main_box.has_css_class("details-widget"));
    }

    #[test]
    #[ignore = "Requires GTK main thread - run with --test-threads=1"]
    fn test_render_details_with_css_classes() {
        init_gtk_for_tests();
        let menu_store = Rc::new(create_menu_store());
        let callback: ActionCallback = Rc::new(|_id, _action| {});
        let renderer = WidgetRenderer::new(menu_store.clone(), callback.clone());

        let summary = Widget::Label {
            text: "Summary".to_string(),
            css_classes: vec![],
        };

        let content = Widget::Label {
            text: "Content".to_string(),
            css_classes: vec![],
        };

        let widget = render_details(
            &renderer,
            &callback,
            &menu_store,
            &summary,
            &content,
            &["custom-class".to_string(), "another-class".to_string()],
            &dummy_action(),
            "custom_details",
        );

        let main_box: gtk::Box = widget.downcast().unwrap();
        assert!(main_box.has_css_class("custom-class"));
        assert!(main_box.has_css_class("another-class"));
    }

    #[test]
    #[ignore = "Requires GTK main thread - run with --test-threads=1"]
    fn test_render_details_complex_summary() {
        init_gtk_for_tests();
        let menu_store = Rc::new(create_menu_store());
        let callback: ActionCallback = Rc::new(|_id, _action| {});
        let renderer = WidgetRenderer::new(menu_store.clone(), callback.clone());

        let summary = Widget::Row {
            spacing: 8,
            css_classes: vec![],
            children: vec![
                Node::keyed(
                    "icon",
                    Widget::Label {
                        text: "📄".to_string(),
                        css_classes: vec![],
                    },
                ),
                Node::keyed(
                    "text",
                    Widget::Label {
                        text: "Document details".to_string(),
                        css_classes: vec![],
                    },
                ),
            ],
        };

        let content = Widget::Label {
            text: "Full document content here".to_string(),
            css_classes: vec![],
        };

        let widget = render_details(
            &renderer,
            &callback,
            &menu_store,
            &summary,
            &content,
            &[],
            &dummy_action(),
            "complex_details",
        );

        assert!(widget.is::<gtk::Box>());
    }

    #[test]
    #[ignore = "Requires GTK main thread - run with --test-threads=1"]
    fn test_details_widget_menu_integration() {
        init_gtk_for_tests();
        let menu_store = Rc::new(create_menu_store());
        let callback: ActionCallback = Rc::new(|_id, _action| {});
        let renderer = WidgetRenderer::new(menu_store.clone(), callback.clone());

        let summary = Widget::Label {
            text: "Click to expand".to_string(),
            css_classes: vec![],
        };

        let content = Widget::Label {
            text: "Hidden content".to_string(),
            css_classes: vec![],
        };

        let widget = render_details(
            &renderer,
            &callback,
            &menu_store,
            &summary,
            &content,
            &[],
            &dummy_action(),
            "menu_test_details",
        );

        let main_box: gtk::Box = widget.downcast().unwrap();
        let summary_button = main_box.first_child().unwrap();
        let summary_button: gtk::Button = summary_button.downcast().unwrap();

        // Initially collapsed
        let content_revealer = main_box.last_child().unwrap();
        let content_revealer: gtk::Revealer = content_revealer.downcast().unwrap();
        assert!(!content_revealer.reveals_child());

        // Click to expand
        summary_button.emit_clicked();

        // Should now be expanded (menu store updated)
        let state = menu_store.get_state();
        assert_eq!(state.active_menu_id, Some(menu_id_for_widget("menu_test_details")));
    }

    #[test]
    #[ignore = "Requires GTK main thread - run with --test-threads=1"]
    fn test_details_reconcile_updates() {
        init_gtk_for_tests();
        let menu_store = Rc::new(create_menu_store());

        let old_widget = waft_ipc::Widget::Details {
            summary: Box::new(Widget::Label {
                text: "Old summary".to_string(),
                css_classes: vec![],
            }),
            content: Box::new(Widget::Label {
                text: "Old content".to_string(),
                css_classes: vec![],
            }),
            css_classes: vec![],
            on_toggle: dummy_action(),
        };

        let new_widget = waft_ipc::Widget::Details {
            summary: Box::new(Widget::Label {
                text: "New summary".to_string(),
                css_classes: vec![],
            }),
            content: Box::new(Widget::Label {
                text: "New content".to_string(),
                css_classes: vec![],
            }),
            css_classes: vec![],
            on_toggle: dummy_action(),
        };

        let summary_gtk = gtk::Label::new(Some("Old summary"));
        let content_gtk = gtk::Label::new(Some("Old content"));

        let details = DetailsWidget::new(
            DetailsProps {
                menu_id: "test_menu".to_string(),
            },
            summary_gtk.upcast(),
            content_gtk.upcast(),
            &[],
            menu_store,
        );

        let outcome = details.try_reconcile(&old_widget, &new_widget);
        assert_eq!(
            outcome,
            crate::reconcile::ReconcileOutcome::Updated,
            "Changing summary/content should update in-place (reconciler handles children)"
        );
    }

    #[test]
    #[ignore = "Requires GTK main thread - run with --test-threads=1"]
    fn test_details_reconcile_recreate_on_action_change() {
        init_gtk_for_tests();
        let menu_store = Rc::new(create_menu_store());

        let old_widget = waft_ipc::Widget::Details {
            summary: Box::new(Widget::Label {
                text: "Summary".to_string(),
                css_classes: vec![],
            }),
            content: Box::new(Widget::Label {
                text: "Content".to_string(),
                css_classes: vec![],
            }),
            css_classes: vec![],
            on_toggle: Action {
                id: "old_action".to_string(),
                params: ActionParams::None,
            },
        };

        let new_widget = waft_ipc::Widget::Details {
            summary: Box::new(Widget::Label {
                text: "Summary".to_string(),
                css_classes: vec![],
            }),
            content: Box::new(Widget::Label {
                text: "Content".to_string(),
                css_classes: vec![],
            }),
            css_classes: vec![],
            on_toggle: Action {
                id: "new_action".to_string(),
                params: ActionParams::None,
            },
        };

        let summary_gtk = gtk::Label::new(Some("Summary"));
        let content_gtk = gtk::Label::new(Some("Content"));

        let details = DetailsWidget::new(
            DetailsProps {
                menu_id: "test_menu".to_string(),
            },
            summary_gtk.upcast(),
            content_gtk.upcast(),
            &[],
            menu_store,
        );

        let outcome = details.try_reconcile(&old_widget, &new_widget);
        assert_eq!(
            outcome,
            crate::reconcile::ReconcileOutcome::Recreate,
            "Changing action should recreate"
        );
    }

    #[test]
    #[ignore = "Requires GTK main thread - run with --test-threads=1"]
    fn test_details_reconcile_recreate_on_css_change() {
        init_gtk_for_tests();
        let menu_store = Rc::new(create_menu_store());

        let old_widget = waft_ipc::Widget::Details {
            summary: Box::new(Widget::Label {
                text: "Summary".to_string(),
                css_classes: vec![],
            }),
            content: Box::new(Widget::Label {
                text: "Content".to_string(),
                css_classes: vec![],
            }),
            css_classes: vec!["old-class".to_string()],
            on_toggle: dummy_action(),
        };

        let new_widget = waft_ipc::Widget::Details {
            summary: Box::new(Widget::Label {
                text: "Summary".to_string(),
                css_classes: vec![],
            }),
            content: Box::new(Widget::Label {
                text: "Content".to_string(),
                css_classes: vec![],
            }),
            css_classes: vec!["new-class".to_string()],
            on_toggle: dummy_action(),
        };

        let summary_gtk = gtk::Label::new(Some("Summary"));
        let content_gtk = gtk::Label::new(Some("Content"));

        let details = DetailsWidget::new(
            DetailsProps {
                menu_id: "test_menu".to_string(),
            },
            summary_gtk.upcast(),
            content_gtk.upcast(),
            &["old-class".to_string()],
            menu_store,
        );

        let outcome = details.try_reconcile(&old_widget, &new_widget);
        assert_eq!(
            outcome,
            crate::reconcile::ReconcileOutcome::Recreate,
            "Changing CSS classes should recreate"
        );
    }
}
