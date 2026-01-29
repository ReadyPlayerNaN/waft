//! Pure GTK4 Expandable Feature Toggle widget.
//!
//! A toggle button with icon, title, details, and an expand button for menus.

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;

use gtk::prelude::*;
use uuid::Uuid;

use super::menu_chevron::{MenuChevronProps, MenuChevronWidget};
use crate::menu_state::{MenuOp, MenuStore};

/// Properties for initializing an expandable feature toggle.
#[derive(Debug, Clone)]
pub struct FeatureToggleExpandableProps {
    pub active: bool,
    pub busy: bool,
    pub expanded: bool,
    pub details: Option<String>,
    pub icon: String,
    pub title: String,
}

/// Output events from the expandable feature toggle.
#[derive(Debug, Clone)]
pub enum FeatureToggleExpandableOutput {
    Activate,
    Deactivate,
    ToggleExpand,
}

/// Pure GTK4 expandable feature toggle widget.
#[derive(Clone)]
pub struct FeatureToggleExpandableWidget {
    pub root: gtk::Box,
    #[allow(dead_code)]
    main_button: gtk::Button,
    menu_chevron: MenuChevronWidget,
    icon_image: gtk::Image,
    #[allow(dead_code)]
    title_label: gtk::Label,
    details_label: gtk::Label,
    details_revealer: gtk::Revealer,
    active: Rc<RefCell<bool>>,
    busy: Rc<RefCell<bool>>,
    #[allow(dead_code)]
    expanded: Rc<RefCell<bool>>,
    on_output: Rc<RefCell<Option<Box<dyn Fn(FeatureToggleExpandableOutput)>>>>,
    on_expand: Rc<RefCell<Option<Box<dyn Fn(bool)>>>>,
    pub menu_id: String,
}

impl FeatureToggleExpandableWidget {
    /// Create a new expandable feature toggle widget.
    pub fn new(props: FeatureToggleExpandableProps, menu_store: Arc<MenuStore>) -> Self {
        // Generate unique ID for this menu
        let menu_id = Uuid::new_v4().to_string();
        let root = gtk::Box::builder()
            .orientation(gtk::Orientation::Horizontal)
            .spacing(0)
            .css_classes(["feature-toggle-expandable"])
            .build();

        // Main button (toggle on/off)
        let main_button = gtk::Button::builder()
            .hexpand(true)
            .css_classes(["toggle-main"])
            .build();

        let main_content = gtk::Box::builder()
            .orientation(gtk::Orientation::Horizontal)
            .spacing(12)
            .valign(gtk::Align::Center)
            .build();

        let icon_image = gtk::Image::builder()
            .icon_name(&props.icon)
            .pixel_size(24)
            .build();

        let text_content = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .valign(gtk::Align::Center)
            .spacing(2)
            .build();

        let title_label = gtk::Label::builder()
            .label(&props.title)
            .css_classes(["heading", "title"])
            .xalign(0.0)
            .build();

        let details_revealer = gtk::Revealer::builder()
            .transition_type(gtk::RevealerTransitionType::SlideDown)
            .reveal_child(props.details.is_some())
            .build();

        let details_label = gtk::Label::builder()
            .label(props.details.as_deref().unwrap_or(""))
            .css_classes(["dim-label", "caption"])
            .xalign(0.0)
            .build();

        details_revealer.set_child(Some(&details_label));

        text_content.append(&title_label);
        text_content.append(&details_revealer);

        main_content.append(&icon_image);
        main_content.append(&text_content);

        main_button.set_child(Some(&main_content));
        let menu_chevron = MenuChevronWidget::new(MenuChevronProps {
            expanded: props.expanded,
        });
        let expand_button = gtk::Button::builder()
            .css_classes(["toggle-expand"])
            .build();
        expand_button.set_child(menu_chevron.widget());

        root.append(&main_button);
        root.append(&expand_button);

        let active = Rc::new(RefCell::new(props.active));
        let busy = Rc::new(RefCell::new(props.busy));
        let expanded = Rc::new(RefCell::new(props.expanded));
        let on_output: Rc<RefCell<Option<Box<dyn Fn(FeatureToggleExpandableOutput)>>>> =
            Rc::new(RefCell::new(None));
        let on_expand: Rc<RefCell<Option<Box<dyn Fn(bool)>>>> = Rc::new(RefCell::new(None));

        // Update CSS classes based on initial state
        Self::update_css_classes(&root, props.active, props.busy, props.expanded);

        // Connect main button click handler
        let active_ref = active.clone();
        let on_output_ref = on_output.clone();
        main_button.connect_clicked(move |_| {
            let is_active = *active_ref.borrow();
            if let Some(ref callback) = *on_output_ref.borrow() {
                if is_active {
                    callback(FeatureToggleExpandableOutput::Deactivate);
                } else {
                    callback(FeatureToggleExpandableOutput::Activate);
                }
            }
        });

        // Connect menu chevron click handler
        let menu_store_clone = menu_store.clone();
        let menu_id_clone = menu_id.clone();
        expand_button.connect_clicked(move |_| {
            // Always emit OpenMenu - MenuStore will handle toggle logic
            menu_store_clone.emit(MenuOp::OpenMenu(menu_id_clone.clone()));
        });

        // Subscribe to menu store updates
        let root_clone = root.clone();
        let menu_chevron_clone = menu_chevron.clone();
        let expanded_clone = expanded.clone();
        let active_clone = active.clone();
        let busy_clone = busy.clone();
        let menu_store_clone = menu_store.clone();
        let menu_id_clone = menu_id.clone();
        let on_expand_clone = on_expand.clone();
        menu_store.subscribe(move || {
            let state = menu_store_clone.get_state();
            let should_be_open = state.active_menu_id.as_ref() == Some(&menu_id_clone);

            *expanded_clone.borrow_mut() = should_be_open;
            menu_chevron_clone.set_expanded(should_be_open);
            Self::update_css_classes(
                &root_clone,
                *active_clone.borrow(),
                *busy_clone.borrow(),
                should_be_open,
            );

            // Notify plugin of expand state change
            if let Some(ref callback) = *on_expand_clone.borrow() {
                callback(should_be_open);
            }
        });

        // Sync initial state
        {
            let state = menu_store.get_state();
            let should_be_open = state.active_menu_id.as_ref() == Some(&menu_id);
            *expanded.borrow_mut() = should_be_open;
            menu_chevron.set_expanded(should_be_open);
            Self::update_css_classes(&root, *active.borrow(), *busy.borrow(), should_be_open);
        }

        Self {
            root,
            main_button,
            menu_chevron,
            icon_image,
            title_label,
            details_label,
            details_revealer,
            active,
            busy,
            expanded,
            on_output,
            on_expand,
            menu_id,
        }
    }

    /// Set the callback for expand state changes.
    pub fn set_expand_callback<F>(&self, callback: F)
    where
        F: Fn(bool) + 'static,
    {
        *self.on_expand.borrow_mut() = Some(Box::new(callback));
    }

    /// Set the callback for output events.
    pub fn connect_output<F>(&self, callback: F)
    where
        F: Fn(FeatureToggleExpandableOutput) + 'static,
    {
        *self.on_output.borrow_mut() = Some(Box::new(callback));
    }

    /// Update the active state.
    pub fn set_active(&self, active: bool) {
        *self.active.borrow_mut() = active;
        Self::update_css_classes(
            &self.root,
            active,
            *self.busy.borrow(),
            *self.expanded.borrow(),
        );
    }

    /// Update the busy state.
    pub fn set_busy(&self, busy: bool) {
        *self.busy.borrow_mut() = busy;
        Self::update_css_classes(
            &self.root,
            *self.active.borrow(),
            busy,
            *self.expanded.borrow(),
        );
    }

    /// Update the details text.
    pub fn set_details(&self, details: Option<String>) {
        self.details_revealer.set_reveal_child(details.is_some());
        self.details_label
            .set_label(details.as_deref().unwrap_or(""));
    }

    /// Update the icon.
    #[allow(dead_code)]
    pub fn set_icon(&self, icon: &str) {
        self.icon_image.set_icon_name(Some(icon));
    }

    /// Get a reference to the root widget.
    pub fn widget(&self) -> gtk::Widget {
        self.root.clone().upcast::<gtk::Widget>()
    }

    fn update_css_classes(container: &gtk::Box, active: bool, busy: bool, expanded: bool) {
        // Remove all state classes first
        container.remove_css_class("active");
        container.remove_css_class("busy");
        container.remove_css_class("expanded");

        // Add state classes
        if active {
            container.add_css_class("active");
        }
        if busy {
            container.add_css_class("busy");
        }
        if expanded {
            container.add_css_class("expanded");
        }
    }
}
