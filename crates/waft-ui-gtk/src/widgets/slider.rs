//! Stateful Slider widget — icon button, horizontal scale, and optional expand button.
//!
//! Provides `set_value()`, `set_muted()`, `set_icon()`, `set_expandable()` for
//! in-place property updates without recreating the GTK tree.

use crate::renderer::{ActionCallback, WidgetRenderer};
use crate::widgets::icon::IconWidget;
use crate::menu_state::{is_menu_open, menu_id_for_widget, toggle_menu};
use gtk::glib;
use gtk::prelude::*;
use std::cell::RefCell;
use std::rc::Rc;
use waft_core::menu_state::MenuStore;
use waft_core::{Callback, VoidCallback};
use waft_ipc::widget::{Action, ActionParams};

/// Properties for initializing a slider.
#[derive(Debug, Clone)]
pub struct SliderProps {
    pub icon: String,
    pub value: f64,
    pub muted: bool,
    pub expandable: bool,
    /// Optional deterministic menu ID. When provided, the slider uses this
    /// instead of generating a random UUID. Callers should use
    /// `menu_id_for_widget(widget_id)` to produce a stable ID that
    /// matches the content revealer created by the reconciler/renderer.
    pub menu_id: Option<String>,
}

/// Stateful GTK slider widget with icon button, scale, and optional expand button.
#[derive(Clone)]
pub struct SliderWidget {
    pub(crate) root: gtk::Box,
    icon_widget: IconWidget,
    icon_button: gtk::Button,
    scale: gtk::Scale,
    expand_revealer: gtk::Revealer,
    base_icon: Rc<RefCell<String>>,
    value: Rc<RefCell<f64>>,
    muted: Rc<RefCell<bool>>,
    expandable: Rc<RefCell<bool>>,
    on_value_change: Callback<f64>,
    on_icon_click: VoidCallback,
    scale_handler_id: Rc<RefCell<Option<glib::SignalHandlerId>>>,
    pub menu_id: Option<String>,
}

impl SliderWidget {
    pub fn new(props: SliderProps, menu_store: Option<Rc<MenuStore>>) -> Self {
        let menu_id = menu_store.as_ref().map(|_| {
            props.menu_id.clone().unwrap_or_else(|| uuid::Uuid::new_v4().to_string())
        });

        // Main vertical container
        let root = gtk::Box::new(gtk::Orientation::Vertical, 0);

        // Top horizontal box with controls
        let controls_box = gtk::Box::new(gtk::Orientation::Horizontal, 8);

        // Icon button
        let icon_button = gtk::Button::new();
        icon_button.add_css_class("flat");
        icon_button.add_css_class("circular");

        let icon_name = muted_icon_name(&props.icon, props.muted);
        let icon_widget = IconWidget::from_name(&icon_name, 24);
        icon_button.set_child(Some(icon_widget.widget()));

        // Scale (slider)
        let adjustment = gtk::Adjustment::new(
            props.value * 100.0,
            0.0,
            100.0,
            1.0,
            10.0,
            0.0,
        );

        let scale = gtk::Scale::new(gtk::Orientation::Horizontal, Some(&adjustment));
        scale.set_draw_value(false);
        scale.set_hexpand(true);

        controls_box.append(&icon_button);
        controls_box.append(&scale);

        // Expand button in revealer
        let expand_revealer = gtk::Revealer::builder()
            .transition_type(gtk::RevealerTransitionType::SlideLeft)
            .transition_duration(200)
            .reveal_child(props.expandable)
            .build();

        if let Some(ref store) = menu_store {
            let expand_button = gtk::Button::new();
            expand_button.add_css_class("flat");
            expand_button.add_css_class("circular");

            let mid = menu_id.clone().unwrap();
            let is_open = is_menu_open(store, &mid);
            let chevron_icon = if is_open { "pan-up-symbolic" } else { "pan-down-symbolic" };
            let chevron_widget = IconWidget::from_name(chevron_icon, 16);
            expand_button.set_child(Some(chevron_widget.widget()));

            let store_clone = store.clone();
            let mid_clone = mid.clone();
            expand_button.connect_clicked(move |_| {
                toggle_menu(&store_clone, &mid_clone);
            });

            expand_revealer.set_child(Some(&expand_button));
        }

        controls_box.append(&expand_revealer);
        root.append(&controls_box);

        if props.muted {
            root.add_css_class("slider-row-muted");
        }

        let base_icon = Rc::new(RefCell::new(props.icon));
        let value = Rc::new(RefCell::new(props.value));
        let muted = Rc::new(RefCell::new(props.muted));
        let expandable = Rc::new(RefCell::new(props.expandable));
        let on_value_change: Callback<f64> = Rc::new(RefCell::new(None));
        let on_icon_click: VoidCallback = Rc::new(RefCell::new(None));

        // Connect icon button click
        let on_icon_click_ref = on_icon_click.clone();
        icon_button.connect_clicked(move |_| {
            if let Some(ref callback) = *on_icon_click_ref.borrow() {
                callback();
            }
        });

        // Connect scale value change — store handler ID so set_value() can block it
        let on_value_change_ref = on_value_change.clone();
        let handler_id = scale.connect_value_changed(move |s| {
            let v = s.value() / 100.0;
            if let Some(ref callback) = *on_value_change_ref.borrow() {
                callback(v);
            }
        });
        let scale_handler_id = Rc::new(RefCell::new(Some(handler_id)));

        Self {
            root,
            icon_widget,
            icon_button,
            scale,
            expand_revealer,
            base_icon,
            value,
            muted,
            expandable,
            on_value_change,
            on_icon_click,
            scale_handler_id,
            menu_id,
        }
    }

    /// Set the callback for value changes.
    pub fn connect_value_change<F>(&self, callback: F)
    where
        F: Fn(f64) + 'static,
    {
        *self.on_value_change.borrow_mut() = Some(Box::new(callback));
    }

    /// Set the callback for icon button clicks.
    pub fn connect_icon_click<F>(&self, callback: F)
    where
        F: Fn() + 'static,
    {
        *self.on_icon_click.borrow_mut() = Some(Box::new(callback));
    }

    /// Update the scale value, blocking the signal handler to prevent feedback loops.
    pub fn set_value(&self, v: f64) {
        *self.value.borrow_mut() = v;
        if let Some(ref handler_id) = *self.scale_handler_id.borrow() {
            self.scale.block_signal(handler_id);
            self.scale.set_value(v * 100.0);
            self.scale.unblock_signal(handler_id);
        }
    }

    /// Update the muted state — toggles CSS class and icon suffix.
    pub fn set_muted(&self, m: bool) {
        *self.muted.borrow_mut() = m;
        if m {
            self.root.add_css_class("slider-row-muted");
        } else {
            self.root.remove_css_class("slider-row-muted");
        }
        let base = self.base_icon.borrow().clone();
        let name = muted_icon_name(&base, m);
        self.icon_widget.set_icon(&name);
    }

    /// Update the base icon name.
    pub fn set_icon(&self, icon: &str) {
        *self.base_icon.borrow_mut() = icon.to_string();
        let m = *self.muted.borrow();
        let name = muted_icon_name(icon, m);
        self.icon_widget.set_icon(&name);
    }

    /// Update the expandable state.
    pub fn set_expandable(&self, expandable: bool) {
        *self.expandable.borrow_mut() = expandable;
        self.expand_revealer.set_reveal_child(expandable);
    }

    /// Get a reference to the root widget.
    pub fn widget(&self) -> gtk::Widget {
        self.root.clone().upcast::<gtk::Widget>()
    }
}

impl crate::reconcile::Reconcilable for SliderWidget {
    fn try_reconcile(
        &self,
        old_desc: &waft_ipc::Widget,
        new_desc: &waft_ipc::Widget,
    ) -> crate::reconcile::ReconcileOutcome {
        use crate::reconcile::ReconcileOutcome;
        match (old_desc, new_desc) {
            (
                waft_ipc::Widget::Slider {
                    on_value_change: old_vc,
                    on_icon_click: old_ic,
                    ..
                },
                waft_ipc::Widget::Slider {
                    icon,
                    value,
                    muted,
                    expandable,
                    on_value_change: new_vc,
                    on_icon_click: new_ic,
                    ..
                },
            ) => {
                if old_vc != new_vc || old_ic != new_ic {
                    return ReconcileOutcome::Recreate;
                }
                self.set_value(*value);
                self.set_muted(*muted);
                self.set_icon(icon);
                self.set_expandable(*expandable);
                ReconcileOutcome::Updated
            }
            _ => ReconcileOutcome::Recreate,
        }
    }
}

/// Compute the icon name, appending "-muted" when muted.
fn muted_icon_name(base: &str, muted: bool) -> String {
    if muted {
        format!("{}-muted", base.trim_end_matches("-symbolic"))
    } else {
        base.to_string()
    }
}

/// Render a Slider widget from the IPC protocol using SliderWidget.
///
/// This bridges the daemon widget protocol to the stateful SliderWidget,
/// ensuring daemon plugins and cdylib plugins use the same rendering.
#[allow(clippy::too_many_arguments)]
pub(crate) fn render_slider(
    renderer: &WidgetRenderer,
    callback: &ActionCallback,
    menu_store: &Rc<MenuStore>,
    icon: &str,
    value: f64,
    muted: bool,
    expandable: bool,
    expanded_content: &Option<Box<crate::types::Widget>>,
    on_value_change: &Action,
    on_icon_click: &Action,
    widget_id: &str,
) -> gtk::Widget {
    let mid = menu_id_for_widget(widget_id);
    let slider = SliderWidget::new(
        SliderProps {
            icon: icon.to_string(),
            value,
            muted,
            expandable,
            menu_id: Some(mid.clone()),
        },
        Some(menu_store.clone()),
    );

    // Wire up action callbacks
    let cb = callback.clone();
    let wid = widget_id.to_string();
    let action = on_value_change.clone();
    slider.connect_value_change(move |v| {
        let mut a = action.clone();
        a.params = ActionParams::Value(v);
        cb(wid.clone(), a);
    });

    let cb = callback.clone();
    let wid = widget_id.to_string();
    let action = on_icon_click.clone();
    slider.connect_icon_click(move || {
        cb(wid.clone(), action.clone());
    });

    // Revealer for expanded content (created outside the SliderWidget because it
    // needs the WidgetRenderer for recursive rendering)
    if expandable {
        if let Some(content) = expanded_content {
            let revealer = gtk::Revealer::new();
            revealer.set_transition_type(gtk::RevealerTransitionType::SlideDown);
            revealer.set_transition_duration(200);

            let content_id = format!("{}:expanded", widget_id);
            let gtk_content = renderer.render(content, &content_id);
            revealer.set_child(Some(&gtk_content));

            let is_open = is_menu_open(menu_store, &mid);
            revealer.set_reveal_child(is_open);

            // Subscribe to MenuStore so the revealer reacts to expand button clicks
            let store_clone = menu_store.clone();
            let mid_clone = mid.clone();
            let revealer_clone = revealer.clone();
            menu_store.subscribe(move || {
                let state = store_clone.get_state();
                let should_be_open = state.active_menu_id.as_deref() == Some(mid_clone.as_str());
                revealer_clone.set_reveal_child(should_be_open);
            });

            // Append revealer to the root box
            let root: gtk::Box = slider.root.clone();
            root.append(&revealer);
        }
    }

    slider.widget()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{ActionParams, Widget};
    use std::cell::RefCell;
    use waft_core::menu_state::create_menu_store;

    fn init_gtk() {
        use std::sync::Once;
        static INIT: Once = Once::new();
        INIT.call_once(|| {
            gtk::init().expect("Failed to initialize GTK");
        });
    }

    #[test]
    #[ignore = "Requires GTK main thread - run with --test-threads=1"]
    fn test_render_slider_basic() {
        init_gtk();
        let menu_store = Rc::new(create_menu_store());
        let callback: ActionCallback = Rc::new(|_id, _action| {});
        let renderer = WidgetRenderer::new(menu_store.clone(), callback.clone());

        let on_value_change = Action {
            id: "set_volume".to_string(),
            params: ActionParams::Value(0.5),
        };
        let on_icon_click = Action {
            id: "toggle_mute".to_string(),
            params: ActionParams::None,
        };

        let widget = render_slider(
            &renderer,
            &callback,
            &menu_store,
            "audio-volume-high-symbolic",
            0.5,
            false,
            false,
            &None,
            &on_value_change,
            &on_icon_click,
            "audio_slider",
        );

        assert!(widget.is::<gtk::Box>());
        let main_box: gtk::Box = widget.downcast().unwrap();
        assert_eq!(main_box.orientation(), gtk::Orientation::Vertical);
    }

    #[test]
    #[ignore = "Requires GTK main thread - run with --test-threads=1"]
    fn test_render_slider_muted() {
        init_gtk();
        let menu_store = Rc::new(create_menu_store());
        let callback: ActionCallback = Rc::new(|_id, _action| {});
        let renderer = WidgetRenderer::new(menu_store.clone(), callback.clone());

        let on_value_change = Action {
            id: "set_volume".to_string(),
            params: ActionParams::Value(0.0),
        };
        let on_icon_click = Action {
            id: "toggle_mute".to_string(),
            params: ActionParams::None,
        };

        let widget = render_slider(
            &renderer,
            &callback,
            &menu_store,
            "audio-volume-high-symbolic",
            0.0,
            true,
            false,
            &None,
            &on_value_change,
            &on_icon_click,
            "muted_slider",
        );

        let main_box: gtk::Box = widget.downcast().unwrap();
        assert!(main_box.has_css_class("slider-row-muted"));
    }

    #[test]
    #[ignore = "Requires GTK main thread - run with --test-threads=1"]
    fn test_render_slider_expandable_collapsed() {
        init_gtk();
        let menu_store = Rc::new(create_menu_store());
        let callback: ActionCallback = Rc::new(|_id, _action| {});
        let renderer = WidgetRenderer::new(menu_store.clone(), callback.clone());

        let expanded_content = Some(Box::new(Widget::Label {
            text: "Expanded Content".to_string(),
            css_classes: vec![],
        }));

        let on_value_change = Action {
            id: "set_value".to_string(),
            params: ActionParams::Value(0.75),
        };
        let on_icon_click = Action {
            id: "icon_click".to_string(),
            params: ActionParams::None,
        };

        let widget = render_slider(
            &renderer,
            &callback,
            &menu_store,
            "preferences-system-symbolic",
            0.75,
            false,
            true,
            &expanded_content,
            &on_value_change,
            &on_icon_click,
            "expandable_slider",
        );

        assert!(widget.is::<gtk::Box>());
    }

    #[test]
    #[ignore = "Requires GTK main thread - run with --test-threads=1"]
    fn test_render_slider_icon_click_callback() {
        init_gtk();
        let menu_store = Rc::new(create_menu_store());

        let captured_actions: Rc<RefCell<Vec<(String, Action)>>> =
            Rc::new(RefCell::new(Vec::new()));
        let captured_actions_clone = captured_actions.clone();

        let callback: ActionCallback = Rc::new(move |widget_id, action| {
            captured_actions_clone
                .borrow_mut()
                .push((widget_id, action));
        });

        let renderer = WidgetRenderer::new(menu_store.clone(), callback.clone());

        let on_value_change = Action {
            id: "set_value".to_string(),
            params: ActionParams::Value(0.5),
        };
        let on_icon_click = Action {
            id: "icon_clicked".to_string(),
            params: ActionParams::None,
        };

        let widget = render_slider(
            &renderer,
            &callback,
            &menu_store,
            "audio-volume-high-symbolic",
            0.5,
            false,
            false,
            &None,
            &on_value_change,
            &on_icon_click,
            "test_slider",
        );

        let main_box: gtk::Box = widget.downcast().unwrap();
        let controls_box = main_box.first_child().unwrap();
        let controls_box: gtk::Box = controls_box.downcast().unwrap();
        let icon_button = controls_box.first_child().unwrap();
        let icon_button: gtk::Button = icon_button.downcast().unwrap();

        icon_button.emit_clicked();

        let actions = captured_actions.borrow();
        assert_eq!(actions.len(), 1);
        assert_eq!(actions[0].0, "test_slider");
        assert_eq!(actions[0].1.id, "icon_clicked");
    }

    #[test]
    #[ignore = "Requires GTK main thread - run with --test-threads=1"]
    fn test_render_slider_value_range() {
        init_gtk();
        let menu_store = Rc::new(create_menu_store());
        let callback: ActionCallback = Rc::new(|_id, _action| {});
        let renderer = WidgetRenderer::new(menu_store.clone(), callback.clone());

        let on_value_change = Action {
            id: "set_value".to_string(),
            params: ActionParams::Value(0.0),
        };
        let on_icon_click = Action {
            id: "icon_click".to_string(),
            params: ActionParams::None,
        };

        let widget_min = render_slider(
            &renderer, &callback, &menu_store, "icon", 0.0, false, false,
            &None, &on_value_change, &on_icon_click, "slider_min",
        );
        assert!(widget_min.is::<gtk::Box>());

        let widget_max = render_slider(
            &renderer, &callback, &menu_store, "icon", 1.0, false, false,
            &None, &on_value_change, &on_icon_click, "slider_max",
        );
        assert!(widget_max.is::<gtk::Box>());

        let widget_mid = render_slider(
            &renderer, &callback, &menu_store, "icon", 0.5, false, false,
            &None, &on_value_change, &on_icon_click, "slider_mid",
        );
        assert!(widget_mid.is::<gtk::Box>());
    }

    #[test]
    #[ignore = "Requires GTK main thread - run with --test-threads=1"]
    fn test_slider_widget_set_value() {
        init_gtk();
        let menu_store = Rc::new(create_menu_store());

        let slider = SliderWidget::new(
            SliderProps {
                icon: "audio-volume-high-symbolic".to_string(),
                value: 0.5,
                muted: false,
                expandable: false,
                menu_id: None,
            },
            Some(menu_store),
        );

        // set_value should not trigger the callback
        let called = Rc::new(RefCell::new(false));
        let called_clone = called.clone();
        slider.connect_value_change(move |_| {
            *called_clone.borrow_mut() = true;
        });

        slider.set_value(0.75);
        assert!(!*called.borrow(), "set_value should block the signal handler");
        assert!((slider.scale.value() - 75.0).abs() < 0.01);
    }

    #[test]
    #[ignore = "Requires GTK main thread - run with --test-threads=1"]
    fn test_slider_widget_set_muted() {
        init_gtk();
        let menu_store = Rc::new(create_menu_store());

        let slider = SliderWidget::new(
            SliderProps {
                icon: "audio-volume-high-symbolic".to_string(),
                value: 0.5,
                muted: false,
                expandable: false,
                menu_id: None,
            },
            Some(menu_store),
        );

        assert!(!slider.root.has_css_class("slider-row-muted"));
        slider.set_muted(true);
        assert!(slider.root.has_css_class("slider-row-muted"));
        slider.set_muted(false);
        assert!(!slider.root.has_css_class("slider-row-muted"));
    }

    #[test]
    #[ignore = "Requires GTK main thread - run with --test-threads=1"]
    fn test_slider_widget_set_icon() {
        init_gtk();
        let menu_store = Rc::new(create_menu_store());

        let slider = SliderWidget::new(
            SliderProps {
                icon: "audio-volume-high-symbolic".to_string(),
                value: 0.5,
                muted: false,
                expandable: false,
                menu_id: None,
            },
            Some(menu_store),
        );

        slider.set_icon("brightness-display-symbolic");
        assert_eq!(*slider.base_icon.borrow(), "brightness-display-symbolic");
    }

    #[test]
    #[ignore = "Requires GTK main thread - run with --test-threads=1"]
    fn test_slider_widget_set_expandable() {
        init_gtk();
        let menu_store = Rc::new(create_menu_store());

        let slider = SliderWidget::new(
            SliderProps {
                icon: "icon".to_string(),
                value: 0.5,
                muted: false,
                expandable: false,
                menu_id: None,
            },
            Some(menu_store),
        );

        assert!(!slider.expand_revealer.reveals_child());
        slider.set_expandable(true);
        assert!(slider.expand_revealer.reveals_child());
    }

    #[test]
    fn test_muted_icon_name() {
        assert_eq!(muted_icon_name("audio-volume-high-symbolic", false), "audio-volume-high-symbolic");
        assert_eq!(muted_icon_name("audio-volume-high-symbolic", true), "audio-volume-high-muted");
        assert_eq!(muted_icon_name("microphone", false), "microphone");
        assert_eq!(muted_icon_name("microphone", true), "microphone-muted");
    }
}
