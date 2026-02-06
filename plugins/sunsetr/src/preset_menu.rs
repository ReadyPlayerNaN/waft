//! Sunsetr preset menu widget.
//!
//! Displays a list of available sunsetr presets.

use std::cell::RefCell;
use std::rc::Rc;

use gtk::prelude::*;
use waft_plugin_api::common::Callback;
use waft_plugin_api::ui::icon::IconWidget;

/// A single preset menu item with an optional checkmark.
#[derive(Clone)]
struct PresetMenuItem {
    root: gtk::Button,
    checkmark: gtk::Image,
}

impl PresetMenuItem {
    fn new(label: &str, is_active: bool) -> Self {
        let hbox = gtk::Box::builder()
            .orientation(gtk::Orientation::Horizontal)
            .spacing(12)
            .build();

        // Icon on left
        let icon = IconWidget::from_name("preferences-system-symbolic", 16);
        hbox.append(icon.widget());

        // Label in middle
        let label_widget = gtk::Label::builder()
            .label(label)
            .hexpand(true)
            .halign(gtk::Align::Start)
            .build();
        hbox.append(&label_widget);

        // Checkmark on right
        let checkmark = gtk::Image::builder()
            .icon_name("object-select-symbolic")
            .pixel_size(16)
            .visible(is_active)
            .build();
        hbox.append(&checkmark);

        let button = gtk::Button::builder()
            .css_classes(["menu-item"])
            .child(&hbox)
            .build();

        Self {
            root: button,
            checkmark,
        }
    }

    fn set_active(&self, active: bool) {
        self.checkmark.set_visible(active);
    }

    fn widget(&self) -> &gtk::Button {
        &self.root
    }
}

/// Output events from the preset menu.
#[derive(Debug, Clone)]
pub enum PresetMenuOutput {
    SelectPreset(String), // Select a named preset
    SelectDefault,        // Clear preset, return to default
}

/// Sunsetr preset menu widget.
pub struct PresetMenuWidget {
    pub root: gtk::Box,
    items_container: gtk::Box,
    on_output: Callback<PresetMenuOutput>,
    menu_items: RefCell<Vec<(Option<String>, PresetMenuItem)>>, // (preset_name, item) - None for "Default"
}

impl PresetMenuWidget {
    /// Create a new preset menu widget.
    pub fn new() -> Self {
        let root = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .css_classes(["preset-menu"])
            .build();

        let items_container = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .build();

        root.append(&items_container);

        let on_output: Callback<PresetMenuOutput> = Rc::new(RefCell::new(None));

        Self {
            root,
            items_container,
            on_output,
            menu_items: RefCell::new(Vec::new()),
        }
    }

    /// Set the callback for output events.
    pub fn connect_output<F>(&self, callback: F)
    where
        F: Fn(PresetMenuOutput) + 'static,
    {
        *self.on_output.borrow_mut() = Some(Box::new(callback));
    }

    /// Update the list of presets.
    pub fn set_presets(&self, presets: Vec<String>, active_preset: Option<String>) {
        // Remove all existing items
        while let Some(child) = self.items_container.first_child() {
            self.items_container.remove(&child);
        }

        if presets.is_empty() {
            // Show "no presets" message
            let no_presets_label = gtk::Label::builder()
                .label(waft_plugin_api::i18n::t("sunsetr-no-presets"))
                .css_classes(["dim-label", "caption"])
                .margin_top(8)
                .margin_bottom(8)
                .margin_start(12)
                .margin_end(12)
                .build();
            self.items_container.append(&no_presets_label);
            *self.menu_items.borrow_mut() = Vec::new();
            return;
        }

        let mut menu_items = Vec::new();

        // Add "Default" option first
        let default_item = PresetMenuItem::new("Default", active_preset.is_none());
        let default_button = default_item.widget();
        default_button.connect_clicked({
            let callback = self.on_output.clone();
            move |_| {
                if let Some(ref cb) = *callback.borrow() {
                    cb(PresetMenuOutput::SelectDefault);
                }
            }
        });
        self.items_container.append(default_button);
        menu_items.push((None, default_item));

        // Add regular presets
        for preset in presets {
            let is_active = active_preset.as_ref() == Some(&preset);
            let item = PresetMenuItem::new(&preset, is_active);
            let button = item.widget();

            button.connect_clicked({
                let preset = preset.clone();
                let callback = self.on_output.clone();
                move |_| {
                    if let Some(ref cb) = *callback.borrow() {
                        cb(PresetMenuOutput::SelectPreset(preset.clone()));
                    }
                }
            });

            self.items_container.append(button);
            menu_items.push((Some(preset), item));
        }

        *self.menu_items.borrow_mut() = menu_items;
    }

    /// Update which preset is marked as active without rebuilding the menu.
    pub fn update_active_preset(&self, active_preset: Option<String>) {
        let items = self.menu_items.borrow();

        for (preset_name, item) in items.iter() {
            let is_active = match (preset_name, &active_preset) {
                (None, None) => true,        // Default is active when no preset
                (Some(name), Some(active)) => name == active, // Preset matches
                _ => false,
            };
            item.set_active(is_active);
        }
    }

    /// Get a reference to the root widget.
    pub fn widget(&self) -> gtk::Widget {
        self.root.clone().upcast::<gtk::Widget>()
    }
}
