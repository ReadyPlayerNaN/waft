//! Stateful Slider widget — icon button, horizontal scale, and optional expand button.
//!
//! Provides `set_value()`, `set_disabled()`, `set_icon()`, `set_expandable()` for
//! in-place property updates without recreating the GTK tree.

use crate::menu_state::{is_menu_open, toggle_menu};
use crate::icons::IconWidget;
use crate::widgets::menu_chevron::{MenuChevronProps, MenuChevronWidget};
use gtk::glib;
use gtk::prelude::*;
use std::cell::RefCell;
use std::rc::Rc;
use waft_core::menu_state::MenuStore;
use waft_core::{Callback, VoidCallback};

/// Properties for initializing a slider.
#[derive(Debug, Clone)]
pub struct SliderProps {
    pub icon: String,
    pub value: f64,
    pub disabled: bool,
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
    scale: gtk::Scale,
    expand_revealer: gtk::Revealer,
    icon: Rc<RefCell<String>>,
    value: Rc<RefCell<f64>>,
    disabled: Rc<RefCell<bool>>,
    expandable: Rc<RefCell<bool>>,
    on_value_change: Callback<f64>,
    on_value_commit: Callback<f64>,
    on_icon_click: VoidCallback,
    scale_handler_id: Rc<RefCell<Option<glib::SignalHandlerId>>>,
    interacting: Rc<RefCell<bool>>,
    /// Tracks whether the mouse button is physically held down on the scale.
    /// Accessed only inside gesture handler closures.
    #[allow(dead_code)]
    pointer_down: Rc<RefCell<bool>>,
    /// Held to keep the active debounce `SourceId` alive across the widget's
    /// lifetime. Accessed only inside gesture/scroll handler closures.
    #[allow(dead_code)]
    debounce_source: Rc<RefCell<Option<glib::SourceId>>>,
    pub menu_id: Option<String>,
}

/// Cancel any pending debounce and schedule a new one-shot timer.
///
/// When the timer fires, `interacting` is set to `false` and the commit
/// callback is fired with the user's final value. Backend values that arrived
/// during the interaction were suppressed by `set_value()`; they will be
/// applied normally on the next entity update now that `interacting` is false.
fn schedule_interaction_end(
    debounce_source: &Rc<RefCell<Option<glib::SourceId>>>,
    interacting: &Rc<RefCell<bool>>,
    scale: &gtk::Scale,
    on_value_commit: &Callback<f64>,
    delay_ms: u64,
) {
    // Cancel any existing debounce timer
    if let Some(source_id) = debounce_source.borrow_mut().take() {
        source_id.remove();
    }

    let interacting = interacting.clone();
    let scale = scale.clone();
    let debounce_source_inner = debounce_source.clone();
    let on_value_commit = on_value_commit.clone();

    let source_id = glib::timeout_add_local_once(
        std::time::Duration::from_millis(delay_ms),
        move || {
            // Clear the stored source ID since this timer has fired
            *debounce_source_inner.borrow_mut() = None;

            // Read the user's final value before clearing interaction state
            let committed_value = scale.value() / 100.0;

            *interacting.borrow_mut() = false;

            // Fire the commit callback with the value the user settled on
            if let Some(ref callback) = *on_value_commit.borrow() {
                callback(committed_value);
            }
        },
    );

    *debounce_source.borrow_mut() = Some(source_id);
}

impl SliderWidget {
    pub fn new(props: SliderProps, menu_store: Option<Rc<MenuStore>>) -> Self {
        let menu_id = menu_store.as_ref().map(|_| {
            props
                .menu_id
                .clone()
                .unwrap_or_else(|| uuid::Uuid::new_v4().to_string())
        });

        // Main vertical container
        let root = gtk::Box::new(gtk::Orientation::Vertical, 0);
        root.add_css_class("slider-row");

        // Top horizontal box with controls
        let controls_box = gtk::Box::new(gtk::Orientation::Horizontal, 8);

        // Icon button
        let icon_button = gtk::Button::new();
        icon_button.add_css_class("slider-icon");

        let icon_widget = IconWidget::from_name(&props.icon, 24);
        icon_button.set_child(Some(icon_widget.widget()));

        // Scale (slider)
        let adjustment = gtk::Adjustment::new(props.value * 100.0, 0.0, 100.0, 1.0, 10.0, 0.0);

        let scale = gtk::Scale::new(gtk::Orientation::Horizontal, Some(&adjustment));
        scale.set_draw_value(false);
        scale.set_hexpand(true);
        scale.add_css_class("slider-scale");

        let scale_wrapper = gtk::Box::new(gtk::Orientation::Horizontal, 0);
        scale_wrapper.set_hexpand(true);
        scale_wrapper.append(&scale);

        controls_box.append(&icon_button);
        controls_box.append(&scale_wrapper);

        // Expand button in revealer
        let expand_revealer = gtk::Revealer::builder()
            .transition_type(gtk::RevealerTransitionType::SlideLeft)
            .transition_duration(200)
            .reveal_child(props.expandable)
            .build();

        if let Some(ref store) = menu_store {
            let expand_button = gtk::Button::new();
            expand_button.add_css_class("slider-expand");

            let mid = menu_id.clone().unwrap();
            let is_open = is_menu_open(store, &mid);
            let menu_chevron = MenuChevronWidget::new(MenuChevronProps { expanded: is_open });
            expand_button.set_child(Some(&menu_chevron.root));

            let store_clone = store.clone();
            let mid_clone = mid.clone();
            expand_button.connect_clicked(move |_| {
                toggle_menu(&store_clone, &mid_clone);
            });

            // Subscribe to MenuStore so chevron updates when menu opens/closes
            let store_sub = store.clone();
            let mid_sub = mid.clone();
            let chevron_sub = menu_chevron.clone();
            store.subscribe(move || {
                let state = store_sub.get_state();
                let should_be_open = state.active_menu_id.as_deref() == Some(mid_sub.as_str());
                chevron_sub.set_expanded(should_be_open);
            });

            expand_revealer.set_child(Some(&expand_button));
        }

        controls_box.append(&expand_revealer);
        root.append(&controls_box);

        crate::css::toggle_class(&root, "disabled", props.disabled);

        let icon = Rc::new(RefCell::new(props.icon));
        let value = Rc::new(RefCell::new(props.value));
        let disabled = Rc::new(RefCell::new(props.disabled));
        let expandable = Rc::new(RefCell::new(props.expandable));
        let on_value_change: Callback<f64> = Rc::new(RefCell::new(None));
        let on_value_commit: Callback<f64> = Rc::new(RefCell::new(None));
        let on_icon_click: VoidCallback = Rc::new(RefCell::new(None));

        // Connect icon button click
        let on_icon_click_ref = on_icon_click.clone();
        icon_button.connect_clicked(move |_| {
            if let Some(ref callback) = *on_icon_click_ref.borrow() {
                callback();
            }
        });

        // Interaction tracking state
        let interacting = Rc::new(RefCell::new(false));
        let pointer_down = Rc::new(RefCell::new(false));
        let debounce_source: Rc<RefCell<Option<glib::SourceId>>> = Rc::new(RefCell::new(None));

        // Connect scale value change -- store handler ID so set_value() can block it.
        // The handler also marks interaction for keyboard-driven changes (arrow keys).
        let scale_handler_id: Rc<RefCell<Option<glib::SignalHandlerId>>> =
            Rc::new(RefCell::new(None));

        let on_value_change_ref = on_value_change.clone();
        let on_value_commit_vc = on_value_commit.clone();
        let interacting_vc = interacting.clone();
        let pointer_down_vc = pointer_down.clone();
        let debounce_source_vc = debounce_source.clone();
        let scale_vc = scale.clone();

        let handler_id = scale.connect_value_changed(move |s| {
            let v = s.value() / 100.0;
            // Only set interacting + schedule debounce for keyboard-driven changes.
            // Pointer drags are tracked by the GestureClick pressed/released lifecycle.
            if !*pointer_down_vc.borrow() {
                *interacting_vc.borrow_mut() = true;
                schedule_interaction_end(
                    &debounce_source_vc,
                    &interacting_vc,
                    &scale_vc,
                    &on_value_commit_vc,
                    200,
                );
            }
            if let Some(ref callback) = *on_value_change_ref.borrow() {
                callback(v);
            }
        });
        *scale_handler_id.borrow_mut() = Some(handler_id);

        // GestureClick for press/release detection on the scale
        let gesture_click = gtk::GestureClick::new();

        let interacting_pressed = interacting.clone();
        let pointer_down_pressed = pointer_down.clone();
        let debounce_source_pressed = debounce_source.clone();
        gesture_click.connect_pressed(move |_, _, _, _| {
            *pointer_down_pressed.borrow_mut() = true;
            *interacting_pressed.borrow_mut() = true;
            // Cancel any pending debounce -- user is actively pressing
            if let Some(source_id) = debounce_source_pressed.borrow_mut().take() {
                source_id.remove();
            }
        });

        let interacting_released = interacting.clone();
        let pointer_down_released = pointer_down.clone();
        let debounce_source_released = debounce_source.clone();
        let scale_released = scale.clone();
        let on_value_commit_released = on_value_commit.clone();
        gesture_click.connect_released(move |_, _, _, _| {
            *pointer_down_released.borrow_mut() = false;
            schedule_interaction_end(
                &debounce_source_released,
                &interacting_released,
                &scale_released,
                &on_value_commit_released,
                100,
            );
        });

        // Handle gesture cancellation (e.g. pointer leaves widget during press)
        let interacting_cancel = interacting.clone();
        let pointer_down_cancel = pointer_down.clone();
        let debounce_source_cancel = debounce_source.clone();
        let scale_cancel = scale.clone();
        let on_value_commit_cancel = on_value_commit.clone();
        gesture_click.connect_cancel(move |_, _| {
            *pointer_down_cancel.borrow_mut() = false;
            schedule_interaction_end(
                &debounce_source_cancel,
                &interacting_cancel,
                &scale_cancel,
                &on_value_commit_cancel,
                100,
            );
        });
        scale_wrapper.add_controller(gesture_click);

        // EventControllerScroll for mousewheel interaction
        let scroll_controller =
            gtk::EventControllerScroll::new(gtk::EventControllerScrollFlags::VERTICAL);

        let interacting_scroll = interacting.clone();
        let debounce_source_scroll = debounce_source.clone();
        let scale_scroll = scale.clone();
        let on_value_commit_scroll = on_value_commit.clone();
        scroll_controller.connect_scroll(move |_, _, _| {
            *interacting_scroll.borrow_mut() = true;
            schedule_interaction_end(
                &debounce_source_scroll,
                &interacting_scroll,
                &scale_scroll,
                &on_value_commit_scroll,
                200,
            );
            glib::Propagation::Proceed
        });
        scale.add_controller(scroll_controller);

        Self {
            root,
            icon_widget,
            scale,
            expand_revealer,
            icon,
            value,
            disabled,
            expandable,
            on_value_change,
            on_value_commit,
            on_icon_click,
            scale_handler_id,
            interacting,
            pointer_down,
            debounce_source,
            menu_id,
        }
    }

    /// Set the callback for value changes (fires on every change during interaction).
    pub fn connect_value_change<F>(&self, callback: F)
    where
        F: Fn(f64) + 'static,
    {
        *self.on_value_change.borrow_mut() = Some(Box::new(callback));
    }

    /// Set the callback for value commits (fires once when interaction ends).
    ///
    /// Unlike `connect_value_change` which fires on every pixel of drag,
    /// this fires only when the user finishes interacting (drag release,
    /// scroll debounce end, keyboard debounce end). Use this for sending
    /// actions to the backend.
    pub fn connect_value_commit<F>(&self, callback: F)
    where
        F: Fn(f64) + 'static,
    {
        *self.on_value_commit.borrow_mut() = Some(Box::new(callback));
    }

    /// Set the callback for icon button clicks.
    pub fn connect_icon_click<F>(&self, callback: F)
    where
        F: Fn() + 'static,
    {
        *self.on_icon_click.borrow_mut() = Some(Box::new(callback));
    }

    /// Update the scale value, blocking the signal handler to prevent feedback loops.
    ///
    /// During active user interaction (drag, scroll, keyboard), backend values are
    /// ignored. When the interaction ends the next entity update will arrive with
    /// the authoritative value and be applied normally.
    pub fn set_value(&self, v: f64) {
        if *self.interacting.borrow() {
            return;
        }
        self.apply_value(v);
    }

    /// Internal: apply a value to the scale, blocking the signal handler.
    fn apply_value(&self, v: f64) {
        *self.value.borrow_mut() = v;
        if let Some(ref handler_id) = *self.scale_handler_id.borrow() {
            self.scale.block_signal(handler_id);
            self.scale.set_value(v * 100.0);
            self.scale.unblock_signal(handler_id);
        }
    }

    /// Update the disabled state — toggles CSS class.
    pub fn set_disabled(&self, d: bool) {
        *self.disabled.borrow_mut() = d;
        crate::css::toggle_class(&self.root, "disabled", d);
    }

    /// Update the icon name.
    pub fn set_icon(&self, icon: &str) {
        *self.icon.borrow_mut() = icon.to_string();
        self.icon_widget.set_icon(icon);
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

impl crate::widget_base::WidgetBase for SliderWidget {
    fn widget(&self) -> gtk::Widget {
        self.widget()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_init::init_gtk_for_tests;
    use std::cell::RefCell;
    use waft_core::menu_state::create_menu_store;

    #[test]
    #[ignore = "Requires GTK main thread - run with --test-threads=1"]
    fn test_slider_widget_set_value() {
        init_gtk_for_tests();
        let menu_store = Rc::new(create_menu_store());

        let slider = SliderWidget::new(
            SliderProps {
                icon: "audio-volume-high-symbolic".to_string(),
                value: 0.5,
                disabled: false,
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
        assert!(
            !*called.borrow(),
            "set_value should block the signal handler"
        );
        assert!((slider.scale.value() - 75.0).abs() < 0.01);
    }

    #[test]
    #[ignore = "Requires GTK main thread - run with --test-threads=1"]
    fn test_slider_widget_set_disabled() {
        init_gtk_for_tests();
        let menu_store = Rc::new(create_menu_store());

        let slider = SliderWidget::new(
            SliderProps {
                icon: "audio-volume-high-symbolic".to_string(),
                value: 0.5,
                disabled: false,
                expandable: false,
                menu_id: None,
            },
            Some(menu_store),
        );

        assert!(!slider.root.has_css_class("disabled"));
        slider.set_disabled(true);
        assert!(slider.root.has_css_class("disabled"));
        slider.set_disabled(false);
        assert!(!slider.root.has_css_class("disabled"));
    }

    #[test]
    #[ignore = "Requires GTK main thread - run with --test-threads=1"]
    fn test_slider_widget_set_icon() {
        init_gtk_for_tests();
        let menu_store = Rc::new(create_menu_store());

        let slider = SliderWidget::new(
            SliderProps {
                icon: "audio-volume-high-symbolic".to_string(),
                value: 0.5,
                disabled: false,
                expandable: false,
                menu_id: None,
            },
            Some(menu_store),
        );

        slider.set_icon("brightness-display-symbolic");
        assert_eq!(*slider.icon.borrow(), "brightness-display-symbolic");
    }

    #[test]
    #[ignore = "Requires GTK main thread - run with --test-threads=1"]
    fn test_slider_widget_set_expandable() {
        init_gtk_for_tests();
        let menu_store = Rc::new(create_menu_store());

        let slider = SliderWidget::new(
            SliderProps {
                icon: "icon".to_string(),
                value: 0.5,
                disabled: false,
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
    #[ignore = "Requires GTK main thread - run with --test-threads=1"]
    fn test_slider_suppresses_value_during_interaction() {
        init_gtk_for_tests();
        let menu_store = Rc::new(create_menu_store());

        let slider = SliderWidget::new(
            SliderProps {
                icon: "audio-volume-high-symbolic".to_string(),
                value: 0.5,
                disabled: false,
                expandable: false,
                menu_id: None,
            },
            Some(menu_store),
        );

        // Simulate interaction start
        *slider.interacting.borrow_mut() = true;

        // Backend pushes a value during interaction
        slider.set_value(0.8);

        // Scale should NOT have moved — backend values are ignored during interaction
        assert!(
            (slider.scale.value() - 50.0).abs() < 0.01,
            "Scale should stay at 50.0 during interaction, got {}",
            slider.scale.value()
        );
    }

    #[test]
    #[ignore = "Requires GTK main thread - run with --test-threads=1"]
    fn test_slider_applies_value_after_interaction_ends() {
        init_gtk_for_tests();
        let menu_store = Rc::new(create_menu_store());

        let slider = SliderWidget::new(
            SliderProps {
                icon: "audio-volume-high-symbolic".to_string(),
                value: 0.5,
                disabled: false,
                expandable: false,
                menu_id: None,
            },
            Some(menu_store),
        );

        // Backend sends a value during interaction — ignored
        *slider.interacting.borrow_mut() = true;
        slider.set_value(0.8);

        // After interaction ends, the next backend value is applied directly
        *slider.interacting.borrow_mut() = false;
        slider.set_value(0.8);

        assert!(
            (slider.scale.value() - 80.0).abs() < 0.01,
            "Scale should be at 80.0 after interaction ends, got {}",
            slider.scale.value()
        );
    }

    #[test]
    #[ignore = "Requires GTK main thread - run with --test-threads=1"]
    fn test_slider_ignores_multiple_backend_values_during_interaction() {
        init_gtk_for_tests();
        let menu_store = Rc::new(create_menu_store());

        let slider = SliderWidget::new(
            SliderProps {
                icon: "audio-volume-high-symbolic".to_string(),
                value: 0.5,
                disabled: false,
                expandable: false,
                menu_id: None,
            },
            Some(menu_store),
        );

        // All backend values during interaction are ignored
        *slider.interacting.borrow_mut() = true;
        slider.set_value(0.6);
        slider.set_value(0.7);
        slider.set_value(0.85);

        // Scale should still be at original position
        assert!(
            (slider.scale.value() - 50.0).abs() < 0.01,
            "Scale should not have moved during interaction"
        );
    }
}
