//! Battery header component.
//!
//! Subscribes to battery entity type and renders battery percentage,
//! state, and icon. Hides when no battery is present.

use std::rc::Rc;

use gtk::prelude::*;

use waft_protocol::entity;
use waft_protocol::entity::power::BatteryState;
use waft_ui_gtk::widgets::info_card::InfoCardWidget;

use waft_client::EntityStore;

/// Displays battery percentage as title, charge state as description.
///
/// Automatically hides when no battery entity exists or when the
/// battery reports `present: false`.
pub struct BatteryComponent {
    container: gtk::Box,
    _widget: Rc<InfoCardWidget>,
}

impl BatteryComponent {
    pub fn new(store: &Rc<EntityStore>) -> Self {
        let container = gtk::Box::builder()
            .orientation(gtk::Orientation::Horizontal)
            .visible(false)
            .build();

        let widget = Rc::new(InfoCardWidget::new("battery-symbolic", "", None));
        container.append(&widget.widget());

        let store_ref = store.clone();
        let widget_ref = widget.clone();
        let container_ref = container.clone();
        store.subscribe_type(entity::power::ENTITY_TYPE, move || {
            let entities =
                store_ref.get_entities_typed::<entity::power::Battery>(entity::power::ENTITY_TYPE);
            match entities.first() {
                Some((_urn, battery)) if battery.present => {
                    widget_ref.set_icon(&battery.icon_name);
                    widget_ref.set_title(&format!("{}%", battery.percentage as u32));
                    widget_ref.set_description(Some(battery_state_label(battery.state)));
                    container_ref.set_visible(true);
                }
                _ => {
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

fn battery_state_label(state: BatteryState) -> &'static str {
    match state {
        BatteryState::Charging => "Charging",
        BatteryState::Discharging => "Discharging",
        BatteryState::FullyCharged => "Fully charged",
        BatteryState::PendingCharge => "Pending charge",
        BatteryState::PendingDischarge => "Pending discharge",
        BatteryState::Empty => "Empty",
        BatteryState::Unknown => "Unknown",
    }
}
