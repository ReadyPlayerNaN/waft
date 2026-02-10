//! Widget reconciler — caches IPC widget descriptions and applies in-place
//! property updates for Reconcilable widgets (FeatureToggle, Slider).
//!
//! Non-reconcilable widgets fall back to remove+add when any property changes.

use std::collections::{HashMap, HashSet};
use std::rc::Rc;

use waft_core::menu_state::MenuStore;
use waft_ipc::{NamedWidget, Widget as IpcWidget};

use crate::reconcile::{ReconcileOutcome, Reconcilable};
use crate::renderer::{ActionCallback, WidgetRenderer};
use crate::widgets::feature_toggle::{FeatureToggleProps, FeatureToggleWidget};
use crate::widgets::info_card::InfoCardWidget;
use crate::widgets::slider::{SliderProps, SliderWidget};
use crate::widgets::status_cycle_button::StatusCycleButtonWidget;

/// What kind of widget was created — the overview uses this to decide
/// which `SlotItem` variant to build.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WidgetKind {
    FeatureToggle,
    Slider,
    InfoCard,
    Generic,
}

/// A reconciled widget ready for the overview to wrap in its own layout types.
pub struct ReconciledWidget {
    pub gtk_widget: gtk::Widget,
    pub kind: WidgetKind,
    pub menu_id: Option<String>,
    /// Rendered expanded_content menu widget (for FeatureToggle).
    pub menu: Option<gtk::Widget>,
    pub id: String,
    pub weight: u32,
}

/// Result of a reconcile pass.
pub struct ReconcileResult {
    /// Newly created widgets to add.
    pub added: Vec<ReconciledWidget>,
    /// IDs of widgets to remove.
    pub removed: Vec<String>,
    /// Whether any structural change occurred (added/removed).
    pub changed: bool,
    /// Number of widgets updated in-place (no add/remove).
    pub updated_in_place: usize,
}

/// Internal cache entry — keeps the IPC description alongside its typed GTK handle.
struct CachedEntry {
    widget_desc: NamedWidget,
    gtk_widget: gtk::Widget,
    kind: WidgetKind,
    menu_id: Option<String>,
    /// Rendered expanded_content menu widget (for FeatureToggle).
    menu: Option<gtk::Widget>,
    typed: Option<Box<dyn Reconcilable>>,
}

/// Caches the last widget description per ID and skips GTK widget creation
/// when the description hasn't changed.
pub struct WidgetReconciler {
    cache: HashMap<String, CachedEntry>,
    menu_store: Rc<MenuStore>,
    action_callback: ActionCallback,
}

impl WidgetReconciler {
    pub fn new(menu_store: Rc<MenuStore>, action_callback: ActionCallback) -> Self {
        Self {
            cache: HashMap::new(),
            menu_store,
            action_callback,
        }
    }

    /// Compare the incoming widget set against the cache.
    ///
    /// - Unchanged widgets are skipped (no GTK work).
    /// - Reconcilable property changes are applied in-place.
    /// - Other changes get a fresh GTK tree; the old ID is removed first.
    /// - Widgets missing from the new set are removed.
    pub fn reconcile(&mut self, new_widgets: &[NamedWidget]) -> ReconcileResult {
        let mut result = ReconcileResult {
            added: Vec::new(),
            removed: Vec::new(),
            changed: false,
            updated_in_place: 0,
        };

        let mut seen_ids: HashSet<String> = HashSet::with_capacity(new_widgets.len());

        for new_widget in new_widgets {
            seen_ids.insert(new_widget.id.clone());

            if let Some(cached) = self.cache.get_mut(&new_widget.id) {
                if cached.widget_desc == *new_widget {
                    continue;
                }
                // Weight change requires re-registration.
                if cached.widget_desc.weight != new_widget.weight {
                    result.removed.push(new_widget.id.clone());
                } else if let Some(ref typed) = cached.typed {
                    match typed.try_reconcile(&cached.widget_desc.widget, &new_widget.widget) {
                        ReconcileOutcome::Updated => {
                            cached.widget_desc = new_widget.clone();
                            result.updated_in_place += 1;
                            continue;
                        }
                        ReconcileOutcome::Recreate => {
                            result.removed.push(new_widget.id.clone());
                        }
                    }
                } else {
                    result.removed.push(new_widget.id.clone());
                }
            }

            let entry = self.create_entry(new_widget);
            result.added.push(ReconciledWidget {
                gtk_widget: entry.gtk_widget.clone(),
                kind: entry.kind,
                menu_id: entry.menu_id.clone(),
                menu: entry.menu.clone(),
                id: new_widget.id.clone(),
                weight: new_widget.weight,
            });
            self.cache.insert(new_widget.id.clone(), entry);
            result.changed = true;
        }

        // Remove stale widgets.
        let stale_ids: Vec<String> = self
            .cache
            .keys()
            .filter(|id| !seen_ids.contains(*id))
            .cloned()
            .collect();

        for id in stale_ids {
            self.cache.remove(&id);
            result.removed.push(id);
            result.changed = true;
        }

        result
    }

    fn create_entry(&self, named_widget: &NamedWidget) -> CachedEntry {
        match &named_widget.widget {
            IpcWidget::FeatureToggle {
                title,
                icon,
                details,
                active,
                busy,
                expandable,
                expanded_content,
                on_toggle,
            } => {
                use crate::menu_state::menu_id_for_widget;
                let toggle = FeatureToggleWidget::new(
                    FeatureToggleProps {
                        title: title.clone(),
                        icon: icon.clone(),
                        details: details.clone(),
                        active: *active,
                        busy: *busy,
                        expandable: *expandable,
                        menu_id: Some(menu_id_for_widget(&named_widget.id)),
                    },
                    Some(self.menu_store.clone()),
                );

                let cb = self.action_callback.clone();
                let wid = named_widget.id.clone();
                let action = on_toggle.clone();
                toggle.connect_output(move |output| {
                    use crate::widgets::feature_toggle::FeatureToggleOutput;
                    use waft_ipc::widget::ActionParams;
                    let mut a = action.clone();
                    a.params = match output {
                        FeatureToggleOutput::Activate => ActionParams::Value(1.0),
                        FeatureToggleOutput::Deactivate => ActionParams::Value(0.0),
                    };
                    cb(wid.clone(), a);
                });

                // Render expanded_content as a menu widget for FeatureGridWidget
                let menu = if *expandable {
                    if let Some(content) = expanded_content {
                        let renderer = WidgetRenderer::new(
                            self.menu_store.clone(),
                            self.action_callback.clone(),
                        );
                        let content_id = format!("{}:expanded", named_widget.id);
                        Some(renderer.render(content, &content_id))
                    } else {
                        None
                    }
                } else {
                    None
                };

                let gtk_widget = toggle.widget();
                let menu_id = toggle.menu_id.clone();

                CachedEntry {
                    widget_desc: named_widget.clone(),
                    gtk_widget,
                    kind: WidgetKind::FeatureToggle,
                    menu_id,
                    menu,
                    typed: Some(Box::new(toggle)),
                }
            }
            IpcWidget::Slider {
                icon,
                value,
                muted,
                expandable,
                on_value_change,
                on_icon_click,
                expanded_content,
            } => {
                use crate::menu_state::menu_id_for_widget;
                let det_menu_id = menu_id_for_widget(&named_widget.id);
                let slider = SliderWidget::new(
                    SliderProps {
                        icon: icon.clone(),
                        value: *value,
                        muted: *muted,
                        expandable: *expandable,
                        menu_id: Some(det_menu_id.clone()),
                    },
                    Some(self.menu_store.clone()),
                );

                let cb = self.action_callback.clone();
                let wid = named_widget.id.clone();
                let action = on_value_change.clone();
                slider.connect_value_change(move |v| {
                    use waft_ipc::widget::ActionParams;
                    let mut a = action.clone();
                    a.params = ActionParams::Value(v);
                    cb(wid.clone(), a);
                });

                let cb = self.action_callback.clone();
                let wid = named_widget.id.clone();
                let action = on_icon_click.clone();
                slider.connect_icon_click(move || {
                    cb(wid.clone(), action.clone());
                });

                // Expanded content revealer (needs renderer for recursive rendering)
                if *expandable {
                    if let Some(content) = expanded_content {
                        let renderer = WidgetRenderer::new(
                            self.menu_store.clone(),
                            self.action_callback.clone(),
                        );
                        let revealer = gtk::Revealer::new();
                        revealer.set_transition_type(gtk::RevealerTransitionType::SlideDown);
                        revealer.set_transition_duration(200);

                        use gtk::prelude::*;
                        let content_id = format!("{}:expanded", named_widget.id);
                        let gtk_content = renderer.render(content, &content_id);
                        revealer.set_child(Some(&gtk_content));

                        use crate::menu_state::is_menu_open;
                        let is_open = is_menu_open(&self.menu_store, &det_menu_id);
                        revealer.set_reveal_child(is_open);

                        // Subscribe to MenuStore so the revealer reacts to expand button clicks
                        let store_clone = self.menu_store.clone();
                        let mid_clone = det_menu_id.clone();
                        let revealer_clone = revealer.clone();
                        self.menu_store.subscribe(move || {
                            let state = store_clone.get_state();
                            let should_be_open = state.active_menu_id.as_deref() == Some(mid_clone.as_str());
                            revealer_clone.set_reveal_child(should_be_open);
                        });

                        slider.root.append(&revealer);
                    }
                }

                let gtk_widget = slider.widget();
                let menu_id = slider.menu_id.clone();

                CachedEntry {
                    widget_desc: named_widget.clone(),
                    gtk_widget,
                    kind: WidgetKind::Slider,
                    menu_id,
                    menu: None,
                    typed: Some(Box::new(slider)),
                }
            }
            IpcWidget::InfoCard {
                icon,
                title,
                description,
                on_click,
            } => {
                let card = match on_click {
                    Some(action) => InfoCardWidget::new_clickable(
                        icon,
                        title,
                        description.as_deref(),
                        &self.action_callback,
                        action,
                        &named_widget.id,
                    ),
                    None => InfoCardWidget::new(icon, title, description.as_deref()),
                };
                let gtk_widget = card.widget();

                CachedEntry {
                    widget_desc: named_widget.clone(),
                    gtk_widget,
                    kind: WidgetKind::InfoCard,
                    menu_id: None,
                    menu: None,
                    typed: Some(Box::new(card)),
                }
            }
            IpcWidget::StatusCycleButton {
                value,
                icon,
                options,
                on_cycle,
            } => {
                let scb = StatusCycleButtonWidget::new(
                    value,
                    icon,
                    options,
                    &self.action_callback,
                    on_cycle,
                    &named_widget.id,
                );
                let gtk_widget = scb.widget();

                CachedEntry {
                    widget_desc: named_widget.clone(),
                    gtk_widget,
                    kind: WidgetKind::Generic,
                    menu_id: None,
                    menu: None,
                    typed: Some(Box::new(scb)),
                }
            }
            _ => {
                let renderer =
                    WidgetRenderer::new(self.menu_store.clone(), self.action_callback.clone());
                let gtk_widget = renderer.render(&named_widget.widget, &named_widget.id);

                CachedEntry {
                    widget_desc: named_widget.clone(),
                    gtk_widget,
                    kind: WidgetKind::Generic,
                    menu_id: None,
                    menu: None,
                    typed: None,
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use waft_ipc::{Action, ActionParams, Widget as IpcWidget};
    use waft_core::menu_state::create_menu_store;

    fn make_label(id: &str, text: &str) -> NamedWidget {
        NamedWidget {
            id: id.to_string(),
            weight: 10,
            widget: IpcWidget::Label {
                text: text.to_string(),
                css_classes: vec![],
            },
        }
    }

    fn make_toggle(id: &str, active: bool) -> NamedWidget {
        NamedWidget {
            id: id.to_string(),
            weight: 100,
            widget: IpcWidget::FeatureToggle {
                title: "Test".to_string(),
                icon: "test-icon".to_string(),
                details: None,
                active,
                busy: false,
                expandable: false,
                expanded_content: None,
                on_toggle: Action {
                    id: "toggle".to_string(),
                    params: ActionParams::None,
                },
            },
        }
    }

    fn make_toggle_full(
        id: &str,
        active: bool,
        busy: bool,
        details: Option<&str>,
        action_id: &str,
        weight: u32,
    ) -> NamedWidget {
        NamedWidget {
            id: id.to_string(),
            weight,
            widget: IpcWidget::FeatureToggle {
                title: "Test".to_string(),
                icon: "test-icon".to_string(),
                details: details.map(|s| s.to_string()),
                active,
                busy,
                expandable: false,
                expanded_content: None,
                on_toggle: Action {
                    id: action_id.to_string(),
                    params: ActionParams::None,
                },
            },
        }
    }

    fn make_slider(id: &str, value: f64, muted: bool) -> NamedWidget {
        NamedWidget {
            id: id.to_string(),
            weight: 50,
            widget: IpcWidget::Slider {
                icon: "audio-volume-high-symbolic".to_string(),
                value,
                muted,
                expandable: false,
                expanded_content: None,
                on_value_change: Action {
                    id: "set_volume".to_string(),
                    params: ActionParams::None,
                },
                on_icon_click: Action {
                    id: "toggle_mute".to_string(),
                    params: ActionParams::None,
                },
            },
        }
    }

    fn init_gtk() {
        use std::sync::Once;
        static INIT: Once = Once::new();
        INIT.call_once(|| {
            gtk::init().expect("Failed to initialize GTK");
        });
    }

    fn make_reconciler() -> WidgetReconciler {
        let menu_store = Rc::new(create_menu_store());
        let action_callback: ActionCallback = Rc::new(|_id, _action| {});
        WidgetReconciler::new(menu_store, action_callback)
    }

    #[test]
    #[ignore = "Requires GTK main thread - run with --test-threads=1"]
    fn test_first_reconcile_adds_all() {
        init_gtk();
        let mut rec = make_reconciler();

        let widgets = vec![make_label("w1", "Hello"), make_label("w2", "World")];
        let result = rec.reconcile(&widgets);

        assert!(result.changed);
        assert_eq!(result.added.len(), 2);
        assert!(result.removed.is_empty());
        assert_eq!(result.updated_in_place, 0);
    }

    #[test]
    #[ignore = "Requires GTK main thread - run with --test-threads=1"]
    fn test_identical_widgets_no_change() {
        init_gtk();
        let mut rec = make_reconciler();

        let widgets = vec![make_label("w1", "Hello")];
        rec.reconcile(&widgets);

        let result = rec.reconcile(&widgets);
        assert!(!result.changed);
        assert!(result.added.is_empty());
        assert!(result.removed.is_empty());
        assert_eq!(result.updated_in_place, 0);
    }

    #[test]
    #[ignore = "Requires GTK main thread - run with --test-threads=1"]
    fn test_label_change_recreates() {
        init_gtk();
        let mut rec = make_reconciler();

        let widgets = vec![make_label("w1", "Old")];
        rec.reconcile(&widgets);

        let updated = vec![make_label("w1", "New")];
        let result = rec.reconcile(&updated);

        assert!(result.changed);
        assert_eq!(result.added.len(), 1);
        assert_eq!(result.removed.len(), 1);
        assert_eq!(result.removed[0], "w1");
        assert_eq!(result.updated_in_place, 0);
    }

    #[test]
    #[ignore = "Requires GTK main thread - run with --test-threads=1"]
    fn test_widget_removal() {
        init_gtk();
        let mut rec = make_reconciler();

        let widgets = vec![make_label("w1", "A"), make_label("w2", "B")];
        rec.reconcile(&widgets);

        let fewer = vec![make_label("w1", "A")];
        let result = rec.reconcile(&fewer);

        assert!(result.changed);
        assert!(result.added.is_empty());
        assert_eq!(result.removed.len(), 1);
        assert_eq!(result.removed[0], "w2");
    }

    #[test]
    #[ignore = "Requires GTK main thread - run with --test-threads=1"]
    fn test_widget_addition() {
        init_gtk();
        let mut rec = make_reconciler();

        let widgets = vec![make_label("w1", "A")];
        rec.reconcile(&widgets);

        let more = vec![make_label("w1", "A"), make_label("w2", "B")];
        let result = rec.reconcile(&more);

        assert!(result.changed);
        assert_eq!(result.added.len(), 1);
        assert!(result.removed.is_empty());
    }

    #[test]
    #[ignore = "Requires GTK main thread - run with --test-threads=1"]
    fn test_feature_toggle_produces_correct_kind() {
        init_gtk();
        let mut rec = make_reconciler();

        let widgets = vec![make_toggle("t1", false)];
        let result = rec.reconcile(&widgets);

        assert_eq!(result.added.len(), 1);
        assert_eq!(result.added[0].kind, WidgetKind::FeatureToggle);
    }

    #[test]
    #[ignore = "Requires GTK main thread - run with --test-threads=1"]
    fn test_toggle_active_change_updates_in_place() {
        init_gtk();
        let mut rec = make_reconciler();

        rec.reconcile(&vec![make_toggle("t1", false)]);
        let result = rec.reconcile(&vec![make_toggle("t1", true)]);

        assert!(!result.changed);
        assert!(result.added.is_empty());
        assert!(result.removed.is_empty());
        assert_eq!(result.updated_in_place, 1);
    }

    #[test]
    #[ignore = "Requires GTK main thread - run with --test-threads=1"]
    fn test_toggle_busy_and_details_update_in_place() {
        init_gtk();
        let mut rec = make_reconciler();

        rec.reconcile(&vec![make_toggle_full("t1", true, false, None, "act", 100)]);
        let result = rec.reconcile(&vec![make_toggle_full(
            "t1", true, true, Some("Connecting..."), "act", 100,
        )]);

        assert!(!result.changed);
        assert_eq!(result.updated_in_place, 1);
    }

    #[test]
    #[ignore = "Requires GTK main thread - run with --test-threads=1"]
    fn test_toggle_action_change_recreates() {
        init_gtk();
        let mut rec = make_reconciler();

        rec.reconcile(&vec![make_toggle_full("t1", true, false, None, "old_action", 100)]);
        let result = rec.reconcile(&vec![make_toggle_full(
            "t1", true, false, None, "new_action", 100,
        )]);

        assert!(result.changed);
        assert_eq!(result.added.len(), 1);
        assert_eq!(result.removed.len(), 1);
        assert_eq!(result.updated_in_place, 0);
    }

    #[test]
    #[ignore = "Requires GTK main thread - run with --test-threads=1"]
    fn test_toggle_weight_change_recreates() {
        init_gtk();
        let mut rec = make_reconciler();

        rec.reconcile(&vec![make_toggle_full("t1", true, false, None, "act", 100)]);
        let result = rec.reconcile(&vec![make_toggle_full("t1", true, false, None, "act", 200)]);

        assert!(result.changed);
        assert_eq!(result.added.len(), 1);
        assert_eq!(result.removed.len(), 1);
        assert_eq!(result.updated_in_place, 0);
    }

    #[test]
    #[ignore = "Requires GTK main thread - run with --test-threads=1"]
    fn test_mixed_in_place_and_structural() {
        init_gtk();
        let mut rec = make_reconciler();

        rec.reconcile(&vec![
            make_toggle("t1", false),
            make_label("w1", "Hello"),
        ]);

        let result = rec.reconcile(&vec![
            make_toggle("t1", true),
            make_label("w1", "World"),
        ]);

        assert!(result.changed);
        assert_eq!(result.added.len(), 1); // label
        assert_eq!(result.removed.len(), 1); // label
        assert_eq!(result.updated_in_place, 1); // toggle
    }

    #[test]
    #[ignore = "Requires GTK main thread - run with --test-threads=1"]
    fn test_slider_produces_correct_kind() {
        init_gtk();
        let mut rec = make_reconciler();

        let widgets = vec![make_slider("s1", 0.5, false)];
        let result = rec.reconcile(&widgets);

        assert_eq!(result.added.len(), 1);
        assert_eq!(result.added[0].kind, WidgetKind::Slider);
    }

    #[test]
    #[ignore = "Requires GTK main thread - run with --test-threads=1"]
    fn test_slider_value_change_updates_in_place() {
        init_gtk();
        let mut rec = make_reconciler();

        rec.reconcile(&vec![make_slider("s1", 0.5, false)]);
        let result = rec.reconcile(&vec![make_slider("s1", 0.75, false)]);

        assert!(!result.changed);
        assert!(result.added.is_empty());
        assert!(result.removed.is_empty());
        assert_eq!(result.updated_in_place, 1);
    }

    #[test]
    #[ignore = "Requires GTK main thread - run with --test-threads=1"]
    fn test_slider_muted_change_updates_in_place() {
        init_gtk();
        let mut rec = make_reconciler();

        rec.reconcile(&vec![make_slider("s1", 0.5, false)]);
        let result = rec.reconcile(&vec![make_slider("s1", 0.5, true)]);

        assert!(!result.changed);
        assert_eq!(result.updated_in_place, 1);
    }

    #[test]
    #[ignore = "Requires GTK main thread - run with --test-threads=1"]
    fn test_slider_menu_id_is_deterministic() {
        init_gtk();
        let mut rec = make_reconciler();

        let widgets = vec![make_slider("audio_output", 0.5, false)];
        let result = rec.reconcile(&widgets);

        assert_eq!(result.added.len(), 1);
        assert_eq!(
            result.added[0].menu_id.as_deref(),
            Some("audio_output_menu"),
            "Slider menu_id should be deterministic based on widget ID"
        );
    }

    #[test]
    #[ignore = "Requires GTK main thread - run with --test-threads=1"]
    fn test_toggle_menu_id_is_deterministic() {
        init_gtk();
        let mut rec = make_reconciler();

        let widgets = vec![make_toggle("bluetooth_toggle", false)];
        let result = rec.reconcile(&widgets);

        assert_eq!(result.added.len(), 1);
        assert_eq!(
            result.added[0].menu_id.as_deref(),
            Some("bluetooth_toggle_menu"),
            "FeatureToggle menu_id should be deterministic based on widget ID"
        );
    }
}
