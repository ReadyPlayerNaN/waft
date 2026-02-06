//! Brightness control widget.
//!
//! Master slider with expandable per-display menu.

use std::cell::RefCell;
use std::rc::Rc;

use gtk::prelude::*;

use super::display_menu::{DisplayMenuOutput, DisplayMenuWidget};
use super::store::{Display, compute_master_average, compute_proportional_scaling};
use crate::common::Callback;
use crate::menu_state::MenuStore;
use crate::ui::slider_control::{SliderControlOutput, SliderControlWidget};

/// Output events from the brightness control widget.
#[derive(Debug, Clone)]
#[allow(dead_code)] // MasterChanged value is informational and may be used in future
pub enum BrightnessControlOutput {
    /// Master slider changed - apply proportional scaling.
    MasterChanged(f64),
    /// Individual display brightness changed.
    DisplayChanged { display_id: String, brightness: f64 },
}

/// Properties for initializing a brightness control widget.
#[derive(Debug, Clone)]
pub struct BrightnessControlProps {
    pub displays: Vec<Display>,
}

/// Combined brightness control widget with master slider and expandable display menu.
pub struct BrightnessControlWidget {
    pub root: gtk::Box,
    slider: SliderControlWidget,
    display_menu: Option<DisplayMenuWidget>,
    displays: Rc<RefCell<Vec<Display>>>,
    master_value: Rc<RefCell<f64>>,
    on_output: Callback<BrightnessControlOutput>,
}

impl BrightnessControlWidget {
    /// Create a new brightness control widget.
    pub fn new(props: BrightnessControlProps, menu_store: Rc<MenuStore>) -> Self {
        let master_value = compute_master_average(&props.displays);
        let has_multiple_displays = props.displays.len() > 1;

        // Create display menu only if multiple displays
        let display_menu = if has_multiple_displays {
            let menu = DisplayMenuWidget::new();
            menu.set_displays(props.displays.clone());
            Some(menu)
        } else {
            None
        };

        // Create slider with or without menu
        let slider = if let Some(ref menu) = display_menu {
            SliderControlWidget::new(
                "display-brightness-symbolic",
                master_value,
                Some(&menu.root),
                menu_store,
            )
        } else {
            SliderControlWidget::new(
                "display-brightness-symbolic",
                master_value,
                None::<&gtk::Box>,
                menu_store,
            )
        };

        let displays = Rc::new(RefCell::new(props.displays));
        let master_value_rc = Rc::new(RefCell::new(master_value));
        let on_output: Callback<BrightnessControlOutput> = Rc::new(RefCell::new(None));

        // Connect slider outputs
        let on_output_ref = on_output.clone();
        let displays_ref = displays.clone();
        let master_value_ref = master_value_rc.clone();
        slider.connect_output(move |event| match event {
            SliderControlOutput::ValueChanged(new_master) => {
                let old_master = *master_value_ref.borrow();

                // Compute proportional scaling
                let displays = displays_ref.borrow();
                let updates = compute_proportional_scaling(&displays, old_master, new_master);
                drop(displays);

                // Update stored master value
                *master_value_ref.borrow_mut() = new_master;

                // Emit master changed event
                if let Some(ref callback) = *on_output_ref.borrow() {
                    callback(BrightnessControlOutput::MasterChanged(new_master));

                    // Also emit individual display changes
                    for (display_id, brightness) in updates {
                        callback(BrightnessControlOutput::DisplayChanged {
                            display_id,
                            brightness,
                        });
                    }
                }
            }
            SliderControlOutput::IconClicked => {
                // No action for icon click (brightness has no mute)
            }
        });

        let root = slider.root.clone();

        let widget = Self {
            root,
            slider,
            display_menu,
            displays,
            master_value: master_value_rc,
            on_output,
        };

        // Connect display menu outputs if present - must be done after widget is created
        // to avoid needing to clone the slider
        widget.connect_menu_outputs();

        widget
    }

    /// Connect the display menu outputs to update master slider.
    fn connect_menu_outputs(&self) {
        if let Some(ref menu) = self.display_menu {
            let on_output_ref = self.on_output.clone();
            let displays_ref = self.displays.clone();
            let master_value_ref = self.master_value.clone();

            // We need to update the slider value when individual displays change.
            // We'll use the scale widget directly from slider_row's children.
            let slider_root = self.slider.root.clone();

            menu.connect_output(move |event| match event {
                DisplayMenuOutput::BrightnessChanged {
                    display_id,
                    brightness,
                } => {
                    // Update display in our list
                    {
                        let mut displays = displays_ref.borrow_mut();
                        if let Some(display) = displays.iter_mut().find(|d| d.id == display_id) {
                            display.brightness = brightness;
                        }
                    }

                    // Recalculate master average
                    let displays = displays_ref.borrow();
                    let new_master = compute_master_average(&displays);
                    drop(displays);

                    *master_value_ref.borrow_mut() = new_master;

                    // Find the scale widget in the slider and update it
                    // The slider_row is the first child, scale is inside it
                    if let Some(slider_row) = slider_root.first_child() {
                        let mut child = slider_row.first_child();
                        while let Some(widget) = child {
                            if let Ok(scale) = widget.clone().downcast::<gtk::Scale>() {
                                scale.set_value(new_master * 100.0);
                                break;
                            }
                            child = widget.next_sibling();
                        }
                    }

                    // Emit the change
                    if let Some(ref callback) = *on_output_ref.borrow() {
                        callback(BrightnessControlOutput::DisplayChanged {
                            display_id,
                            brightness,
                        });
                    }
                }
            });
        }
    }

    /// Set the callback for output events.
    pub fn connect_output<F>(&self, callback: F)
    where
        F: Fn(BrightnessControlOutput) + 'static,
    {
        *self.on_output.borrow_mut() = Some(Box::new(callback));
    }

    /// Update displays and refresh UI.
    pub fn set_displays(&self, displays: Vec<Display>) {
        *self.displays.borrow_mut() = displays.clone();

        // Update master slider
        let master = compute_master_average(&displays);
        *self.master_value.borrow_mut() = master;
        self.slider.set_value(master);

        // Update display menu if present
        if let Some(ref menu) = self.display_menu {
            menu.set_displays(displays);
        }
    }

    /// Update brightness for a specific display (from external source).
    pub fn update_brightness(&self, display_id: &str, brightness: f64) {
        // Update in our list
        {
            let mut displays = self.displays.borrow_mut();
            if let Some(display) = displays.iter_mut().find(|d| d.id == display_id) {
                display.brightness = brightness;
            }
        }

        // Update menu row
        if let Some(ref menu) = self.display_menu {
            menu.update_brightness(display_id, brightness);
        }

        // Recalculate and update master
        let displays = self.displays.borrow();
        let master = compute_master_average(&displays);
        drop(displays);

        *self.master_value.borrow_mut() = master;
        self.slider.set_value(master);
    }
}
