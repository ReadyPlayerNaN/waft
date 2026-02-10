//! Thin wrapper around `waft_ui_gtk::widget_reconciler::WidgetReconciler` that
//! converts `ReconciledWidget` into the overview's `SlotItem` type.

use std::rc::Rc;

use waft_ipc::NamedWidget;
use waft_plugin_api::{Widget, WidgetFeatureToggle};
use waft_ui_gtk::renderer::ActionCallback;
use waft_ui_gtk::widget_reconciler::{WidgetKind, WidgetReconciler};
use waft_core::menu_state::MenuStore;

use crate::plugin_registry::SlotItem;

/// Result of a reconcile pass (overview-specific types).
pub struct ReconcileResult {
    pub added: Vec<SlotItem>,
    pub removed: Vec<String>,
    pub changed: bool,
    pub updated_in_place: usize,
}

/// Delegates reconciliation to waft-ui-gtk and wraps results in `SlotItem`.
pub struct DaemonWidgetReconciler {
    inner: WidgetReconciler,
}

impl DaemonWidgetReconciler {
    pub fn new(menu_store: Rc<MenuStore>, action_callback: ActionCallback) -> Self {
        Self {
            inner: WidgetReconciler::new(menu_store, action_callback),
        }
    }

    pub fn reconcile(&mut self, new_widgets: &[NamedWidget]) -> ReconcileResult {
        let inner_result = self.inner.reconcile(new_widgets);

        let added: Vec<SlotItem> = inner_result
            .added
            .into_iter()
            .map(|rw| match rw.kind {
                WidgetKind::FeatureToggle => SlotItem::Toggle(Rc::new(WidgetFeatureToggle {
                    id: rw.id,
                    weight: rw.weight as i32,
                    el: rw.gtk_widget,
                    menu: None,
                    on_expand_toggled: None,
                    menu_id: rw.menu_id,
                })),
                WidgetKind::Slider | WidgetKind::InfoCard | WidgetKind::Generic => SlotItem::Widget(Rc::new(Widget {
                    id: rw.id,
                    slot: waft_plugin_api::Slot::Info,
                    weight: rw.weight as i32,
                    el: rw.gtk_widget,
                })),
            })
            .collect();

        ReconcileResult {
            added,
            removed: inner_result.removed,
            changed: inner_result.changed,
            updated_in_place: inner_result.updated_in_place,
        }
    }
}
