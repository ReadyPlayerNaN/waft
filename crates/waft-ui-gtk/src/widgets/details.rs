//! Details widget - collapsible details with summary and expandable content

use crate::widgets::menu_chevron::{MenuChevronProps, MenuChevronWidget};
use gtk::prelude::*;
use std::rc::Rc;
use waft_core::menu_state::{MenuOp, MenuStore};

/// Properties for initializing a details widget.
#[derive(Debug, Clone)]
pub struct DetailsProps {
    pub menu_id: String,
}

/// Pure GTK4 details widget with collapsible content.
///
/// Structure:
/// - Root: gtk::Box (Vertical)
///   - Summary row: gtk::Button containing summary widget + menu chevron
///   - Content revealer: gtk::Revealer containing content widget
#[derive(Clone)]
pub struct DetailsWidget {
    pub root: gtk::Box,
    pub menu_id: String,
}

impl DetailsWidget {
    /// Create a new details widget.
    ///
    /// The summary and content are rendered by the caller.
    pub fn new(
        props: DetailsProps,
        summary_gtk: gtk::Widget,
        content_gtk: gtk::Widget,
        css_classes: &[String],
        menu_store: Rc<MenuStore>,
    ) -> Self {
        // Root container: vertical box
        let root = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .spacing(0)
            .css_classes(["details-widget"])
            .build();

        // Apply custom CSS classes
        for css_class in css_classes {
            root.add_css_class(css_class);
        }

        // Summary button (clickable row with chevron)
        let summary_button = gtk::Button::builder()
            .css_classes(["details-summary"])
            .build();

        // Create horizontal container for summary content + chevron
        let summary_container = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        summary_container.set_hexpand(true);

        // Add the summary widget (left side, expands)
        let summary_box = gtk::Box::new(gtk::Orientation::Horizontal, 0);
        summary_box.set_hexpand(true);
        summary_box.append(&summary_gtk);
        summary_container.append(&summary_box);

        // Add the menu chevron (right side)
        let is_open = menu_store
            .get_state()
            .active_menu_id
            .as_ref()
            .map(|id| id == &props.menu_id)
            .unwrap_or(false);
        let menu_chevron = MenuChevronWidget::new(MenuChevronProps { expanded: is_open });
        summary_container.append(&menu_chevron.root);

        summary_button.set_child(Some(&summary_container));

        // Content revealer
        let content_revealer = gtk::Revealer::builder()
            .transition_type(gtk::RevealerTransitionType::SlideDown)
            .transition_duration(200)
            .reveal_child(is_open)
            .build();

        content_revealer.set_child(Some(&content_gtk));

        // Assemble the widget
        root.append(&summary_button);
        root.append(&content_revealer);

        // Connect summary button click handler
        let menu_store_clone = menu_store.clone();
        let menu_id_clone = props.menu_id.clone();
        summary_button.connect_clicked(move |_| {
            menu_store_clone.emit(MenuOp::OpenMenu(menu_id_clone.clone()));
        });

        // Subscribe to menu store updates for expand/collapse
        let content_revealer_clone = content_revealer.clone();
        let menu_chevron_clone = menu_chevron.clone();
        let menu_id_clone = props.menu_id.clone();
        let menu_store_sub = menu_store.clone();
        menu_store.subscribe(move || {
            let state = menu_store_sub.get_state();
            let should_be_open = state.active_menu_id.as_ref() == Some(&menu_id_clone);
            content_revealer_clone.set_reveal_child(should_be_open);
            menu_chevron_clone.set_expanded(should_be_open);
        });

        Self {
            root,
            menu_id: props.menu_id,
        }
    }

    /// Get a reference to the root widget.
    pub fn widget(&self) -> gtk::Widget {
        self.root.clone().upcast::<gtk::Widget>()
    }
}

impl crate::widget_base::WidgetBase for DetailsWidget {
    fn widget(&self) -> gtk::Widget {
        self.widget()
    }
}
