//! Display menu widget for per-display brightness control.
//!
//! Shows a list of displays with individual brightness sliders.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use glib::SignalHandlerId;
use gtk::prelude::*;

use super::store::{Display, DisplayType};
use crate::common::Callback;
use crate::ui::icon::IconWidget;

/// Output events from the display menu.
#[derive(Debug, Clone)]
pub enum DisplayMenuOutput {
    /// User changed brightness for a specific display.
    BrightnessChanged { display_id: String, brightness: f64 },
}

/// A single display row with slider.
struct DisplayRow {
    root: gtk::Box,
    scale: gtk::Scale,
    brightness: Rc<RefCell<f64>>,
    scale_handler_id: SignalHandlerId,
}

impl DisplayRow {
    fn new(display: &Display, on_output: Callback<DisplayMenuOutput>) -> Self {
        let root = gtk::Box::builder()
            .orientation(gtk::Orientation::Horizontal)
            .spacing(8)
            .css_classes(["brightness-display-row"])
            .build();

        // Icon based on display type
        let icon_name = match display.display_type {
            DisplayType::Backlight => "display-brightness-symbolic",
            DisplayType::External => "video-display-symbolic",
        };

        let icon = IconWidget::from_name(icon_name, 16);
        icon.widget().add_css_class("brightness-display-icon");

        // Slider
        let adjustment =
            gtk::Adjustment::new(display.brightness * 100.0, 0.0, 100.0, 1.0, 5.0, 0.0);

        let scale = gtk::Scale::builder()
            .orientation(gtk::Orientation::Horizontal)
            .adjustment(&adjustment)
            .hexpand(true)
            .css_classes(["brightness-display-scale"])
            .build();

        scale.set_draw_value(false);

        // Truncated label
        let label = gtk::Label::builder()
            .label(&display.name)
            .max_width_chars(15)
            .ellipsize(gtk::pango::EllipsizeMode::End)
            .css_classes(["brightness-display-label"])
            .build();

        root.append(icon.widget());
        root.append(&scale);
        root.append(&label);

        let brightness = Rc::new(RefCell::new(display.brightness));
        let brightness_ref = brightness.clone();
        let display_id = display.id.clone();

        let scale_handler_id = scale.connect_value_changed(move |scale| {
            let new_value = scale.value() / 100.0;
            let old_value = *brightness_ref.borrow();

            if (new_value - old_value).abs() > 0.001 {
                *brightness_ref.borrow_mut() = new_value;
                if let Some(ref callback) = *on_output.borrow() {
                    callback(DisplayMenuOutput::BrightnessChanged {
                        display_id: display_id.clone(),
                        brightness: new_value,
                    });
                }
            }
        });

        Self {
            root,
            scale,
            brightness,
            scale_handler_id,
        }
    }

    /// Update the brightness value without emitting a signal.
    fn set_brightness(&self, value: f64) {
        let value = value.clamp(0.0, 1.0);
        *self.brightness.borrow_mut() = value;

        self.scale.block_signal(&self.scale_handler_id);
        self.scale.set_value(value * 100.0);
        self.scale.unblock_signal(&self.scale_handler_id);
    }
}

/// Widget displaying per-display brightness sliders.
pub struct DisplayMenuWidget {
    pub root: gtk::Box,
    rows: Rc<RefCell<HashMap<String, DisplayRow>>>,
    on_output: Callback<DisplayMenuOutput>,
}

impl DisplayMenuWidget {
    pub fn new() -> Self {
        let root = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .spacing(4)
            .css_classes(["brightness-display-menu"])
            .build();

        Self {
            root,
            rows: Rc::new(RefCell::new(HashMap::new())),
            on_output: Rc::new(RefCell::new(None)),
        }
    }

    /// Set the callback for output events.
    pub fn connect_output<F>(&self, callback: F)
    where
        F: Fn(DisplayMenuOutput) + 'static,
    {
        *self.on_output.borrow_mut() = Some(Box::new(callback));
    }

    /// Update the list of displays.
    pub fn set_displays(&self, displays: Vec<Display>) {
        let mut rows = self.rows.borrow_mut();

        // Remove rows for displays that no longer exist
        let current_ids: std::collections::HashSet<String> =
            displays.iter().map(|d| d.id.clone()).collect();

        let removed: Vec<String> = rows
            .keys()
            .filter(|id| !current_ids.contains(*id))
            .cloned()
            .collect();

        for id in removed {
            if let Some(row) = rows.remove(&id) {
                self.root.remove(&row.root);
            }
        }

        // Add or update rows
        for display in displays {
            if let Some(row) = rows.get(&display.id) {
                // Update existing row's brightness
                row.set_brightness(display.brightness);
            } else {
                // Create new row
                let row = DisplayRow::new(&display, self.on_output.clone());
                self.root.append(&row.root);
                rows.insert(display.id.clone(), row);
            }
        }
    }

    /// Update brightness for a specific display.
    #[allow(dead_code)] // Called from BrightnessControlWidget::update_brightness
    pub fn update_brightness(&self, display_id: &str, brightness: f64) {
        let rows = self.rows.borrow();
        if let Some(row) = rows.get(display_id) {
            row.set_brightness(brightness);
        }
    }
}

impl Default for DisplayMenuWidget {
    fn default() -> Self {
        Self::new()
    }
}
