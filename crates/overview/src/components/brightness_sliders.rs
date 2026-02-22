//! Display brightness sliders component.
//!
//! Subscribes to the `display` entity type and renders a brightness slider
//! for every display. No filtering -- all displays get sliders.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use std::time::Duration;

use gtk::prelude::*;

use waft_protocol::Urn;
use waft_protocol::entity;
use waft_ui_gtk::vdom::Component;
use waft_ui_gtk::widgets::slider::{SliderRenderOutput, SliderRenderProps, SliderWidget};

use super::throttled_sender::ThrottledSender;
use waft_client::{EntityActionCallback, EntityStore};

struct SliderEntry {
    widget: Rc<SliderWidget>,
    #[allow(dead_code)]
    throttle: ThrottledSender,
}

/// Renders brightness sliders for all connected displays.
///
/// Each display entity gets its own slider. Sliders are added/removed
/// dynamically as displays appear or disappear.
pub struct BrightnessSlidersComponent {
    container: gtk::Box,
    #[allow(dead_code)]
    sliders: Rc<RefCell<HashMap<String, SliderEntry>>>,
}

impl BrightnessSlidersComponent {
    pub fn new(store: &Rc<EntityStore>, action_callback: &EntityActionCallback) -> Self {
        let container = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .spacing(8)
            .visible(false)
            .build();

        let sliders: Rc<RefCell<HashMap<String, SliderEntry>>> =
            Rc::new(RefCell::new(HashMap::new()));

        let store_ref = store.clone();
        let container_ref = container.clone();
        let sliders_ref = sliders.clone();
        let cb = action_callback.clone();

        store.subscribe_type(entity::display::DISPLAY_ENTITY_TYPE, move || {
            let entities: Vec<(Urn, entity::display::Display)> =
                store_ref.get_entities_typed(entity::display::DISPLAY_ENTITY_TYPE);

            let mut sliders = sliders_ref.borrow_mut();

            // Collect URN strings of current displays
            let current_urns: Vec<String> = entities
                .iter()
                .map(|(urn, _)| urn.as_str().to_string())
                .collect();

            // Remove sliders for displays no longer present
            let stale_keys: Vec<String> = sliders
                .keys()
                .filter(|k| !current_urns.contains(k))
                .cloned()
                .collect();

            for key in stale_keys {
                if let Some(entry) = sliders.remove(&key) {
                    container_ref.remove(&entry.widget.widget());
                }
            }

            // Update existing or create new sliders
            for (urn, display) in &entities {
                let urn_str = urn.as_str().to_string();

                if let Some(entry) = sliders.get(&urn_str) {
                    entry.widget.update(&SliderRenderProps {
                        icon: "display-brightness-symbolic".to_string(),
                        value: display.brightness,
                        disabled: false,
                        expandable: false,
                        expanded: false,
                    });
                } else {
                    let props = SliderRenderProps {
                        icon: "display-brightness-symbolic".to_string(),
                        value: display.brightness,
                        disabled: false,
                        expandable: false,
                        expanded: false,
                    };
                    let slider = Rc::new(SliderWidget::build(&props));

                    // Wire value_change -> throttled set-brightness during drag
                    let throttle = ThrottledSender::new(Duration::from_millis(200));
                    let urn_for_drag = urn.clone();
                    let cb_drag = cb.clone();
                    throttle.set_callback(move |v| {
                        cb_drag(
                            urn_for_drag.clone(),
                            "set-brightness".to_string(),
                            serde_json::json!({ "value": v }),
                        );
                    });
                    let throttle_fn = throttle.throttle_fn();

                    // Wire output events
                    let urn_for_value = urn.clone();
                    let cb_value = cb.clone();
                    slider.connect_output(move |output| match output {
                        SliderRenderOutput::ValueChanged(v) => {
                            throttle_fn(v);
                        }
                        SliderRenderOutput::ValueCommit(v) => {
                            cb_value(
                                urn_for_value.clone(),
                                "set-brightness".to_string(),
                                serde_json::json!({ "value": v }),
                            );
                        }
                        SliderRenderOutput::IconClick | SliderRenderOutput::ExpandClick => {}
                    });

                    container_ref.append(&slider.widget());
                    sliders.insert(urn_str, SliderEntry { widget: slider, throttle });
                }
            }

            container_ref.set_visible(!sliders.is_empty());
        });

        Self { container, sliders }
    }

    pub fn widget(&self) -> &gtk::Widget {
        self.container.upcast_ref()
    }
}
