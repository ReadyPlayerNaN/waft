//! Pure GTK4 Feature Toggle widget.
//!
//! A toggle button with icon, title, and optional details text.

use std::cell::RefCell;
use std::rc::Rc;

use gtk::prelude::*;

/// Properties for initializing a feature toggle.
#[derive(Debug, Clone)]
pub struct FeatureToggleProps {
    pub active: bool,
    pub busy: bool,
    pub details: Option<String>,
    pub icon: String,
    pub title: String,
}

/// Output events from the feature toggle.
#[derive(Debug, Clone)]
pub enum FeatureToggleOutput {
    Activate,
    Deactivate,
}

/// Pure GTK4 feature toggle widget.
pub struct FeatureToggleWidget {
    pub root: gtk::Button,
    icon_image: gtk::Image,
    title_label: gtk::Label,
    details_label: gtk::Label,
    details_revealer: gtk::Revealer,
    active: Rc<RefCell<bool>>,
    busy: Rc<RefCell<bool>>,
    on_output: Rc<RefCell<Option<Box<dyn Fn(FeatureToggleOutput)>>>>,
}

impl FeatureToggleWidget {
    /// Create a new feature toggle widget.
    pub fn new(props: FeatureToggleProps) -> Self {
        let root = gtk::Button::builder().hexpand(true).build();

        let content = gtk::Box::builder()
            .orientation(gtk::Orientation::Horizontal)
            .spacing(12)
            .valign(gtk::Align::Center)
            .build();

        let icon_image = gtk::Image::builder()
            .icon_name(&props.icon)
            .pixel_size(24)
            .height_request(24)
            .build();

        let text_content = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .valign(gtk::Align::Center)
            .spacing(2)
            .css_classes(["text-content"])
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

        content.append(&icon_image);
        content.append(&text_content);

        root.set_child(Some(&content));

        let active = Rc::new(RefCell::new(props.active));
        let busy = Rc::new(RefCell::new(props.busy));
        let on_output: Rc<RefCell<Option<Box<dyn Fn(FeatureToggleOutput)>>>> =
            Rc::new(RefCell::new(None));

        // Update CSS classes based on initial state
        Self::update_css_classes(&root, props.active, props.busy);

        // Connect click handler
        let active_ref = active.clone();
        let on_output_ref = on_output.clone();
        root.connect_clicked(move |_| {
            let is_active = *active_ref.borrow();
            if let Some(ref callback) = *on_output_ref.borrow() {
                if is_active {
                    callback(FeatureToggleOutput::Deactivate);
                } else {
                    callback(FeatureToggleOutput::Activate);
                }
            }
        });

        Self {
            root,
            icon_image,
            title_label,
            details_label,
            details_revealer,
            active,
            busy,
            on_output,
        }
    }

    /// Set the callback for output events.
    pub fn connect_output<F>(&self, callback: F)
    where
        F: Fn(FeatureToggleOutput) + 'static,
    {
        *self.on_output.borrow_mut() = Some(Box::new(callback));
    }

    /// Update the active state.
    pub fn set_active(&self, active: bool) {
        *self.active.borrow_mut() = active;
        Self::update_css_classes(&self.root, active, *self.busy.borrow());
    }

    /// Update the busy state.
    pub fn set_busy(&self, busy: bool) {
        *self.busy.borrow_mut() = busy;
        Self::update_css_classes(&self.root, *self.active.borrow(), busy);
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

    /// Update the title.
    #[allow(dead_code)]
    pub fn set_title(&self, title: &str) {
        self.title_label.set_label(title);
    }

    /// Get a reference to the root widget.
    pub fn widget(&self) -> &gtk::Button {
        &self.root
    }

    fn update_css_classes(button: &gtk::Button, active: bool, busy: bool) {
        // Remove all state classes first
        button.remove_css_class("active");
        button.remove_css_class("busy");

        // Add base class
        if !button.has_css_class("feature-toggle") {
            button.add_css_class("feature-toggle");
        }

        // Add state classes
        if active {
            button.add_css_class("active");
        }
        if busy {
            button.add_css_class("busy");
        }
    }
}
