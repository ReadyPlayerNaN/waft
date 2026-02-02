//! Sunsetr preset menu widget.
//!
//! Displays a list of available sunsetr presets.

use std::cell::RefCell;
use std::rc::Rc;

use gtk::prelude::*;

use crate::ui::menu_item::MenuItemWidget;

/// Output events from the preset menu.
#[derive(Debug, Clone)]
pub enum PresetMenuOutput {
    SelectPreset(String), // preset name
}

/// Sunsetr preset menu widget.
pub struct PresetMenuWidget {
    pub root: gtk::Box,
    items_container: gtk::Box,
    on_output: Rc<RefCell<Option<Box<dyn Fn(PresetMenuOutput)>>>>,
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

        let on_output: Rc<RefCell<Option<Box<dyn Fn(PresetMenuOutput)>>>> =
            Rc::new(RefCell::new(None));

        Self {
            root,
            items_container,
            on_output,
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
    pub fn set_presets(&self, presets: Vec<String>) {
        // Remove all existing items
        while let Some(child) = self.items_container.first_child() {
            self.items_container.remove(&child);
        }

        if presets.is_empty() {
            // Show "no presets" message
            let no_presets_label = gtk::Label::builder()
                .label(&crate::i18n::t("sunsetr-no-presets"))
                .css_classes(["dim-label", "caption"])
                .margin_top(8)
                .margin_bottom(8)
                .margin_start(12)
                .margin_end(12)
                .build();
            self.items_container.append(&no_presets_label);
            return;
        }

        // Create menu items for each preset
        for preset_name in presets {
            let on_output = self.on_output.clone();
            let preset_name_clone = preset_name.clone();

            // Create menu item content
            let content = gtk::Box::builder()
                .orientation(gtk::Orientation::Horizontal)
                .spacing(12)
                .build();

            let icon = gtk::Image::builder()
                .icon_name("preferences-system-symbolic")
                .pixel_size(16)
                .build();

            let label = gtk::Label::builder()
                .label(&preset_name)
                .hexpand(true)
                .xalign(0.0)
                .build();

            content.append(&icon);
            content.append(&label);

            let menu_item = MenuItemWidget::new(&content, move || {
                if let Some(ref callback) = *on_output.borrow() {
                    callback(PresetMenuOutput::SelectPreset(preset_name_clone.clone()));
                }
            });

            self.items_container.append(menu_item.widget());
        }
    }

    /// Get a reference to the root widget.
    pub fn widget(&self) -> gtk::Widget {
        self.root.clone().upcast::<gtk::Widget>()
    }
}
