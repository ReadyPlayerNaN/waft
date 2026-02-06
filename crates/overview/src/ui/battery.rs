//! Pure GTK4 Battery widget.
//!
//! Displays battery icon, charge percentage, and time remaining.

use gtk::prelude::*;

use crate::features::battery::values::BatteryInfo;
use crate::ui::icon::IconWidget;

/// Pure GTK4 battery widget — mirrors the weather widget layout.
pub struct BatteryWidget {
    pub root: gtk::Box,
    icon: IconWidget,
    percentage_label: gtk::Label,
    status_label: gtk::Label,
}

impl BatteryWidget {
    /// Create a new battery widget.
    pub fn new() -> Self {
        let root = gtk::Box::builder()
            .orientation(gtk::Orientation::Horizontal)
            .spacing(8)
            .css_classes(["battery-container"])
            .visible(false)
            .build();

        // Battery icon
        let icon = IconWidget::from_name("battery-symbolic", 32);
        icon.widget().add_css_class("battery-icon");

        // Percentage and status labels
        let labels_box = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .spacing(0)
            .valign(gtk::Align::Center)
            .build();

        let percentage_label = gtk::Label::builder()
            .label("--")
            .xalign(0.0)
            .css_classes(["title-3", "battery-percentage"])
            .build();

        let status_label = gtk::Label::builder()
            .label("")
            .xalign(0.0)
            .css_classes(["dim-label", "battery-status"])
            .build();

        labels_box.append(&percentage_label);
        labels_box.append(&status_label);

        // Content box (icon + labels)
        let content_box = gtk::Box::builder()
            .orientation(gtk::Orientation::Horizontal)
            .spacing(8)
            .build();
        content_box.append(icon.widget());
        content_box.append(&labels_box);

        root.append(&content_box);

        Self {
            root,
            icon,
            percentage_label,
            status_label,
        }
    }

    /// Update the widget with new battery info.
    pub fn update(&self, info: &BatteryInfo) {
        self.root.set_visible(info.present);

        if !info.present {
            return;
        }

        // Update icon
        if !info.icon_name.is_empty() {
            self.icon.set_icon(&info.icon_name);
        }

        // Update percentage text
        self.percentage_label
            .set_label(&format!("{:.0}%", info.percentage));

        // Update status text
        self.status_label.set_label(&info.status_text());
    }
}
