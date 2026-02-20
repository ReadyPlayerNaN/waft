//! Clock header component.
//!
//! Subscribes to clock entity type and renders current time and date
//! via an InfoCardWidget. Hidden until entity data arrives.

use std::rc::Rc;

use gtk::prelude::*;

use waft_protocol::entity;
use waft_ui_gtk::widgets::info_card::InfoCardWidget;

use waft_client::EntityStore;

/// Displays current time as title and date as description.
///
/// Hidden until the first clock entity arrives from the daemon.
pub struct ClockComponent {
    container: gtk::Box,
    _widget: Rc<InfoCardWidget>,
}

impl ClockComponent {
    pub fn new(store: &Rc<EntityStore>) -> Self {
        let container = gtk::Box::builder()
            .orientation(gtk::Orientation::Horizontal)
            .visible(false)
            .build();

        let widget = Rc::new(InfoCardWidget::new("alarm-symbolic", "", None));
        container.append(&widget.widget());

        let store_ref = store.clone();
        let widget_ref = widget.clone();
        let container_ref = container.clone();
        store.subscribe_type(entity::clock::ENTITY_TYPE, move || {
            let entities =
                store_ref.get_entities_typed::<entity::clock::Clock>(entity::clock::ENTITY_TYPE);
            match entities.first() {
                Some((_urn, clock)) => {
                    widget_ref.set_title(&clock.time);
                    widget_ref.set_description(Some(&clock.date));
                    container_ref.set_visible(true);
                }
                None => {
                    container_ref.set_visible(false);
                }
            }
        });

        Self {
            container,
            _widget: widget,
        }
    }

    pub fn widget(&self) -> gtk::Widget {
        self.container.clone().upcast()
    }
}
