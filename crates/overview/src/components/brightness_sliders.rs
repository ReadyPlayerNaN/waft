//! Display brightness sliders component.
//!
//! Subscribes to the `display` entity type and renders a brightness slider
//! for every display. No filtering -- all displays get sliders.

use std::rc::Rc;
use std::time::Duration;

use waft_protocol::entity;
use waft_ui_gtk::vdom::Component;
use waft_ui_gtk::widgets::slider::{SliderRenderOutput, SliderRenderProps, SliderWidget};

use super::entity_keyed_base::{ContainerEntry, EntityKeyedContainer};
use super::throttled_sender::ThrottledSender;
use waft_client::{EntityActionCallback, EntityStore};

struct SliderEntry {
    widget: Rc<SliderWidget>,
    #[allow(dead_code)]
    throttle: ThrottledSender,
}

impl ContainerEntry for SliderEntry {
    fn widget(&self) -> gtk::Widget {
        self.widget.widget()
    }
}

/// Renders brightness sliders for all connected displays.
///
/// Each display entity gets its own slider. Sliders are added/removed
/// dynamically as displays appear or disappear.
pub struct BrightnessSlidersComponent {
    base: EntityKeyedContainer<SliderEntry>,
}

impl BrightnessSlidersComponent {
    pub fn new(store: &Rc<EntityStore>, action_callback: &EntityActionCallback) -> Self {
        let base = EntityKeyedContainer::<SliderEntry>::new(8);
        let (container, entries) = base.refs();

        let store_ref = store.clone();
        let cb = action_callback.clone();

        store.subscribe_type(entity::display::DISPLAY_ENTITY_TYPE, move || {
            let entities: Vec<(waft_protocol::Urn, entity::display::Display)> =
                store_ref.get_entities_typed(entity::display::DISPLAY_ENTITY_TYPE);

            let desired_keys: Vec<String> = entities
                .iter()
                .map(|(urn, _)| urn.as_str().to_string())
                .collect();

            let mut map = entries.borrow_mut();

            EntityKeyedContainer::reconcile(
                &container,
                &mut map,
                &desired_keys,
                |key, entry| {
                    if let Some((_, display)) = entities.iter().find(|(u, _)| u.as_str() == key) {
                        entry.widget.update(&SliderRenderProps {
                            icon: "display-brightness-symbolic".to_string(),
                            value: display.brightness,
                            disabled: false,
                            expandable: false,
                            expanded: false,
                        });
                    }
                },
                |key| {
                    let (urn, display) = entities.iter().find(|(u, _)| u.as_str() == key)?;
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

                    Some(SliderEntry { widget: slider, throttle })
                },
            );
        });

        Self { base }
    }

    pub fn widget(&self) -> &gtk::Widget {
        self.base.widget()
    }
}
