//! Pure GTK4 Feature Toggle widget.
//!
//! A unified toggle button that can be simple or expandable.
//! When expandable=false, only shows the main toggle button.
//! When expandable=true, shows both main button and expand button with menu support.

use std::cell::RefCell;
use std::rc::Rc;

use gtk::prelude::*;
use uuid::Uuid;

use crate::icons::IconWidget;
use crate::vdom::Component;
use crate::widgets::menu_chevron::{MenuChevronProps, MenuChevronWidget};
use waft_core::Callback;
use waft_core::menu_state::{MenuOp, MenuStore};

/// Properties for initializing a feature toggle.
#[derive(Debug, Clone)]
pub struct FeatureToggleProps {
    pub active: bool,
    pub busy: bool,
    pub details: Option<String>,
    pub expandable: bool,
    pub icon: String,
    pub title: String,
    /// Optional deterministic menu ID. When provided, the toggle uses this
    /// instead of generating a random UUID. Callers should use
    /// `menu_id_for_widget(widget_id)` to produce a stable ID that
    /// matches any external content revealer.
    pub menu_id: Option<String>,
}

/// Output events from the feature toggle.
#[derive(Debug, Clone)]
pub enum FeatureToggleOutput {
    Activate,
    Deactivate,
}

/// Pure GTK4 feature toggle widget with optional expandable menu support.
#[derive(Clone)]
pub struct FeatureToggleWidget {
    pub root: gtk::Box,
    expand_revealer: gtk::Revealer,
    icon_widget: IconWidget,
    title_label: gtk::Label,
    details_label: gtk::Label,
    details_revealer: gtk::Revealer,
    active: Rc<RefCell<bool>>,
    busy: Rc<RefCell<bool>>,
    expandable: Rc<RefCell<bool>>,
    expanded: Rc<RefCell<bool>>,
    on_output: Callback<FeatureToggleOutput>,
    on_expand: Callback<bool>,
    pub menu_id: Option<String>,
}

impl FeatureToggleWidget {
    /// Create a new feature toggle widget.
    ///
    /// If menu_store is provided, the widget can be made expandable.
    /// The expand button visibility is controlled by the "expandable" CSS class.
    pub fn new(props: FeatureToggleProps, menu_store: Option<Rc<MenuStore>>) -> Self {
        // Use provided deterministic menu ID, or fall back to random UUID
        let menu_id = menu_store.as_ref().map(|_| {
            props
                .menu_id
                .clone()
                .unwrap_or_else(|| Uuid::new_v4().to_string())
        });

        // Root container: horizontal box containing main button + expand button
        let root = gtk::Box::builder()
            .orientation(gtk::Orientation::Horizontal)
            .spacing(0)
            .hexpand(true)
            .css_classes(["feature-toggle"])
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

        let icon_widget = IconWidget::from_name(&props.icon, 24);
        icon_widget.widget().set_height_request(24);

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

        main_content.append(icon_widget.widget());
        main_content.append(&text_content);

        main_button.set_child(Some(&main_content));

        // Expand button (with menu chevron)
        let menu_chevron = Rc::new(MenuChevronWidget::build(&MenuChevronProps { expanded: false }));
        let expand_button = gtk::Button::builder()
            .css_classes(["toggle-expand"])
            .build();
        expand_button.set_child(Some(&menu_chevron.widget()));

        // Wrap expand button in revealer for smooth slide-left transition
        let expand_revealer = gtk::Revealer::builder()
            .transition_type(gtk::RevealerTransitionType::SlideLeft)
            .transition_duration(200) // 200ms transition
            .reveal_child(props.expandable)
            .build();
        expand_revealer.set_child(Some(&expand_button));

        // Add main button and revealer to root
        root.append(&main_button);
        root.append(&expand_revealer);

        let active = Rc::new(RefCell::new(props.active));
        let busy = Rc::new(RefCell::new(props.busy));
        let expandable = Rc::new(RefCell::new(props.expandable));
        let expanded = Rc::new(RefCell::new(false));
        let on_output: Callback<FeatureToggleOutput> = Rc::new(RefCell::new(None));
        let on_expand: Callback<bool> = Rc::new(RefCell::new(None));

        // Update CSS classes based on initial state
        Self::update_css_classes(&root, props.active, props.busy, props.expandable, false);

        // Connect main button click handler
        let active_ref = active.clone();
        let on_output_ref = on_output.clone();
        main_button.connect_clicked(move |_| {
            let is_active = *active_ref.borrow();
            if let Some(ref callback) = *on_output_ref.borrow() {
                if is_active {
                    callback(FeatureToggleOutput::Deactivate);
                } else {
                    callback(FeatureToggleOutput::Activate);
                }
            }
        });

        // Connect expand button click handler (if menu_store provided)
        if let Some(ref store) = menu_store {
            let menu_store_clone = store.clone();
            let menu_id_clone = menu_id.clone().unwrap();
            expand_button.connect_clicked(move |_| {
                // Always emit OpenMenu - MenuStore will handle toggle logic
                menu_store_clone.emit(MenuOp::OpenMenu(menu_id_clone.clone()));
            });

            // Subscribe to menu store updates
            let root_clone = root.clone();
            let menu_chevron_clone = Rc::clone(&menu_chevron);
            let expanded_clone = expanded.clone();
            let active_clone = active.clone();
            let busy_clone = busy.clone();
            let expandable_clone = expandable.clone();
            let menu_store_clone = store.clone();
            let menu_id_clone = menu_id.clone().unwrap();
            let on_expand_clone = on_expand.clone();
            store.subscribe(move || {
                let state = menu_store_clone.get_state();
                let should_be_open = state.active_menu_id.as_ref() == Some(&menu_id_clone);

                *expanded_clone.borrow_mut() = should_be_open;
                menu_chevron_clone.update(&MenuChevronProps { expanded: should_be_open });
                Self::update_css_classes(
                    &root_clone,
                    *active_clone.borrow(),
                    *busy_clone.borrow(),
                    *expandable_clone.borrow(),
                    should_be_open,
                );

                // Notify plugin of expand state change
                if let Some(ref callback) = *on_expand_clone.borrow() {
                    callback(should_be_open);
                }
            });

            // Sync initial state
            {
                let state = store.get_state();
                let should_be_open =
                    state.active_menu_id.as_ref() == Some(menu_id.as_ref().unwrap());
                *expanded.borrow_mut() = should_be_open;
                menu_chevron.update(&MenuChevronProps { expanded: should_be_open });
                Self::update_css_classes(
                    &root,
                    *active.borrow(),
                    *busy.borrow(),
                    props.expandable,
                    should_be_open,
                );
            }
        }

        Self {
            root,
            expand_revealer,
            icon_widget,
            title_label,
            details_label,
            details_revealer,
            active,
            busy,
            expandable,
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
        F: Fn(FeatureToggleOutput) + 'static,
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
            *self.expandable.borrow(),
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
            *self.expandable.borrow(),
            *self.expanded.borrow(),
        );
    }

    /// Update the expandable state.
    /// When false, the expand button slides out (hidden).
    /// When true, the expand button slides in (visible).
    pub fn set_expandable(&self, expandable: bool) {
        *self.expandable.borrow_mut() = expandable;
        self.expand_revealer.set_reveal_child(expandable);
        Self::update_css_classes(
            &self.root,
            *self.active.borrow(),
            *self.busy.borrow(),
            expandable,
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
    pub fn set_icon(&self, icon: &str) {
        self.icon_widget.set_icon(icon);
    }

    /// Update the title text.
    pub fn set_title(&self, title: &str) {
        self.title_label.set_label(title);
    }

    /// Get a reference to the root widget.
    pub fn widget(&self) -> gtk::Widget {
        self.root.clone().upcast::<gtk::Widget>()
    }

    fn update_css_classes(
        container: &gtk::Box,
        active: bool,
        busy: bool,
        expandable: bool,
        expanded: bool,
    ) {
        crate::css::apply_state_classes(
            container,
            Some("feature-toggle"),
            &[
                ("active", active),
                ("busy", busy),
                ("expandable", expandable),
                ("expanded", expanded),
            ],
        );
    }
}

impl crate::widget_base::WidgetBase for FeatureToggleWidget {
    fn widget(&self) -> gtk::Widget {
        self.widget()
    }
}
