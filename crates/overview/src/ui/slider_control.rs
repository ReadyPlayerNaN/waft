//! Generic slider control widget.
//!
//! A reusable slider with icon button, optional expand button, and optional menu revealer.

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;

use glib::SignalHandlerId;
use gtk::prelude::*;
use uuid::Uuid;

use super::main_window::trigger_window_resize;
use super::menu_chevron::{MenuChevronProps, MenuChevronWidget};
use crate::menu_state::{MenuOp, MenuStore};

/// Output events from the slider control widget.
#[derive(Debug, Clone)]
pub enum SliderControlOutput {
    ValueChanged(f64),
    IconClicked,
}

/// A generic slider control with icon, scale, and optional expandable menu.
pub struct SliderControlWidget {
    pub root: gtk::Box,
    slider_row: gtk::Box,
    icon_image: gtk::Image,
    scale: gtk::Scale,
    _menu_revealer: Option<gtk::Revealer>,
    value: Rc<RefCell<f64>>,
    _expanded: Rc<RefCell<bool>>,
    scale_handler_id: SignalHandlerId,
    on_output: Rc<RefCell<Option<Box<dyn Fn(SliderControlOutput)>>>>,
}

impl SliderControlWidget {
    /// Create a new slider control widget.
    ///
    /// - `icon`: icon name for the icon button
    /// - `value`: initial value (0.0 - 1.0)
    /// - `menu_widget`: optional widget to show in an expandable menu below the slider
    /// - `menu_store`: store for coordinating global menu state
    pub fn new(
        icon: &str,
        value: f64,
        menu_widget: Option<&impl IsA<gtk::Widget>>,
        menu_store: Arc<MenuStore>,
    ) -> Self {
        let root = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .spacing(0)
            .css_classes(["slider-control"])
            .valign(gtk::Align::Start)
            .vexpand(false)
            .build();

        // Slider row
        let slider_row = gtk::Box::builder()
            .orientation(gtk::Orientation::Horizontal)
            .spacing(8)
            .css_classes(["slider-row"])
            .build();

        // Icon button
        let icon_button = gtk::Button::builder().css_classes(["slider-icon"]).build();

        let icon_image = gtk::Image::builder().icon_name(icon).pixel_size(24).build();

        icon_button.set_child(Some(&icon_image));

        // Scale
        let adjustment = gtk::Adjustment::new(value * 100.0, 0.0, 100.0, 1.0, 5.0, 0.0);

        let scale = gtk::Scale::builder()
            .orientation(gtk::Orientation::Horizontal)
            .adjustment(&adjustment)
            .hexpand(true)
            .css_classes(["slider-scale"])
            .build();

        scale.set_draw_value(false);

        slider_row.append(&icon_button);
        slider_row.append(&scale);

        let value_rc = Rc::new(RefCell::new(value));
        let expanded = Rc::new(RefCell::new(false));
        let on_output: Rc<RefCell<Option<Box<dyn Fn(SliderControlOutput)>>>> =
            Rc::new(RefCell::new(None));

        // Generate unique ID for this menu
        let menu_id = Uuid::new_v4().to_string();

        // Optional expand button and menu revealer
        let menu_revealer = if let Some(menu_widget) = menu_widget {
            let expand_button = gtk::Button::builder()
                .css_classes(["slider-expand"])
                .build();

            let menu_chevron = MenuChevronWidget::new(MenuChevronProps { expanded: false });
            expand_button.set_child(menu_chevron.widget());
            slider_row.append(&expand_button);

            let revealer = gtk::Revealer::builder()
                .transition_type(gtk::RevealerTransitionType::SlideDown)
                .reveal_child(false)
                .build();

            let menu_container = gtk::Box::builder()
                .orientation(gtk::Orientation::Vertical)
                .css_classes(["slider-menu-container"])
                .build();

            menu_container.append(menu_widget);
            revealer.set_child(Some(&menu_container));

            // Connect menu chevron click handler
            let expanded_ref = expanded.clone();
            let menu_store_clone = menu_store.clone();
            let menu_id_clone = menu_id.clone();
            expand_button.connect_clicked(move |_| {
                let is_currently_open = *expanded_ref.borrow();
                if is_currently_open {
                    menu_store_clone.emit(MenuOp::CloseMenu(menu_id_clone.clone()));
                } else {
                    menu_store_clone.emit(MenuOp::OpenMenu(menu_id_clone.clone()));
                }
            });

            // Subscribe to menu store updates
            let revealer_ref = revealer.clone();
            let menu_chevron_clone = menu_chevron.clone();
            let slider_row_ref = slider_row.clone();
            let expanded_ref = expanded.clone();
            let menu_store_clone = menu_store.clone();
            let menu_id_clone = menu_id.clone();
            menu_store.subscribe(move || {
                let state = menu_store_clone.get_state();
                let should_be_open = state.active_menu_id.as_ref() == Some(&menu_id_clone);

                *expanded_ref.borrow_mut() = should_be_open;
                menu_chevron_clone.set_expanded(should_be_open);
                revealer_ref.set_reveal_child(should_be_open);

                if should_be_open {
                    slider_row_ref.add_css_class("expanded");
                } else {
                    slider_row_ref.remove_css_class("expanded");
                }

                trigger_window_resize();
            });

            // Sync initial state
            {
                let state = menu_store.get_state();
                let should_be_open = state.active_menu_id.as_ref() == Some(&menu_id);
                *expanded.borrow_mut() = should_be_open;
                menu_chevron.set_expanded(should_be_open);
                revealer.set_reveal_child(should_be_open);
                if should_be_open {
                    slider_row.add_css_class("expanded");
                }
            }

            Some(revealer)
        } else {
            None
        };

        root.append(&slider_row);
        if let Some(ref revealer) = menu_revealer {
            root.append(revealer);
        }

        // Connect icon button click
        let on_output_ref = on_output.clone();
        icon_button.connect_clicked(move |_| {
            if let Some(ref callback) = *on_output_ref.borrow() {
                callback(SliderControlOutput::IconClicked);
            }
        });

        // Connect scale value changed
        let on_output_ref = on_output.clone();
        let value_ref = value_rc.clone();
        let scale_handler_id = scale.connect_value_changed(move |scale| {
            let new_value = scale.value() / 100.0;
            let old_value = *value_ref.borrow();

            if (new_value - old_value).abs() > 0.001 {
                *value_ref.borrow_mut() = new_value;
                if let Some(ref callback) = *on_output_ref.borrow() {
                    callback(SliderControlOutput::ValueChanged(new_value));
                }
            }
        });

        Self {
            root,
            slider_row,
            icon_image,
            scale,
            _menu_revealer: menu_revealer,
            value: value_rc,
            _expanded: expanded,
            scale_handler_id,
            on_output,
        }
    }

    /// Register a callback for output events.
    pub fn connect_output<F>(&self, callback: F)
    where
        F: Fn(SliderControlOutput) + 'static,
    {
        *self.on_output.borrow_mut() = Some(Box::new(callback));
    }

    /// Update the slider value (0.0 - 1.0) without emitting a signal.
    pub fn set_value(&self, value: f64) {
        let value = value.clamp(0.0, 1.0);
        *self.value.borrow_mut() = value;

        self.scale.block_signal(&self.scale_handler_id);
        self.scale.set_value(value * 100.0);
        self.scale.unblock_signal(&self.scale_handler_id);
    }

    /// Update the icon image.
    pub fn set_icon(&self, icon: &str) {
        self.icon_image.set_icon_name(Some(icon));
    }

    /// Access the slider row for adding domain-specific CSS classes.
    pub fn slider_row(&self) -> &gtk::Box {
        &self.slider_row
    }
}
