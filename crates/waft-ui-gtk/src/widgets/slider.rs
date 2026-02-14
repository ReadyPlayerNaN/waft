//! Stateful Slider widget — icon button, horizontal scale, and optional expand button.
//!
//! Provides `set_value()`, `set_disabled()`, `set_icon()`, `set_expandable()` for
//! in-place property updates without recreating the GTK tree.

use crate::menu_state::{is_menu_open, toggle_menu};
use crate::widgets::icon::IconWidget;
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
    on_icon_click: VoidCallback,
    scale_handler_id: Rc<RefCell<Option<glib::SignalHandlerId>>>,
    is_dragging: Rc<RefCell<bool>>,
    pub menu_id: Option<String>,
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
        let on_icon_click: VoidCallback = Rc::new(RefCell::new(None));

        // Connect icon button click
        let on_icon_click_ref = on_icon_click.clone();
        icon_button.connect_clicked(move |_| {
            if let Some(ref callback) = *on_icon_click_ref.borrow() {
                callback();
            }
        });

        // Connect scale value change — store handler ID so set_value() can block it
        let last_user_change: Rc<RefCell<Option<std::time::Instant>>> = Rc::new(RefCell::new(None));
        let is_dragging = Rc::new(RefCell::new(false));
        let on_value_change_ref = on_value_change.clone();
        let last_user_change_ref = last_user_change.clone();
        let is_dragging_ref = is_dragging.clone();
        let handler_id = scale.connect_value_changed(move |s| {
            let v = s.value() / 100.0;
            *last_user_change_ref.borrow_mut() = Some(std::time::Instant::now());
            *is_dragging_ref.borrow_mut() = true;
            if let Some(ref callback) = *on_value_change_ref.borrow() {
                callback(v);
            }
        });
        let scale_handler_id = Rc::new(RefCell::new(Some(handler_id)));

        // Monitor for drag end (no value_changed for 150ms)
        let is_dragging_monitor = is_dragging.clone();
        let last_change_monitor = last_user_change.clone();
        glib::timeout_add_local(std::time::Duration::from_millis(50), move || {
            if let Some(last) = *last_change_monitor.borrow()
                && last.elapsed() > std::time::Duration::from_millis(150)
            {
                *is_dragging_monitor.borrow_mut() = false;
            }
            glib::ControlFlow::Continue
        });

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
            on_icon_click,
            scale_handler_id,
            is_dragging,
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
    ///
    /// Completely blocks external updates during active drag gestures to prevent
    /// value jumping. After the user stops dragging (150ms of inactivity), updates
    /// reconcile immediately.
    pub fn set_value(&self, v: f64) {
        if *self.is_dragging.borrow() {
            return; // Block all external updates during drag
        }
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
    use crate::test_utils::init_gtk_for_tests;
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
}
