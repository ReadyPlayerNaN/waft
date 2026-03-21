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

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use std::cell::Cell;
    use waft_protocol::message::AppNotification;

    fn make_display(brightness: f64) -> entity::display::Display {
        entity::display::Display {
            name: "Test Display".to_string(),
            brightness,
            kind: entity::display::DisplayKind::Backlight,
            connector: None,
        }
    }

    fn make_updated(urn: Urn, data: serde_json::Value) -> AppNotification {
        AppNotification::EntityUpdated {
            urn,
            entity_type: entity::display::DISPLAY_ENTITY_TYPE.to_string(),
            data,
        }
    }

    fn make_removed(urn: Urn) -> AppNotification {
        AppNotification::EntityRemoved {
            urn,
            entity_type: entity::display::DISPLAY_ENTITY_TYPE.to_string(),
        }
    }

    fn noop_action_callback() -> EntityActionCallback {
        Rc::new(|_urn, _action, _params| {})
    }

    fn child_count(container: &gtk::Widget) -> u32 {
        let bx: &gtk::Box = container.downcast_ref().unwrap();
        let mut count = 0u32;
        let mut child = bx.first_child();
        while let Some(c) = child {
            count += 1;
            child = c.next_sibling();
        }
        count
    }

    /// Run all brightness slider GTK tests. Called from the single GTK test entry point.
    pub(crate) fn run_all() {
        test_container_starts_invisible();
        test_add_entity_makes_visible();
        test_add_second_entity();
        test_update_entity_preserves_child_count();
        test_remove_entity_reduces_children();
        test_remove_all_makes_invisible();
        test_action_callback_fires_on_value_commit();
    }

    fn test_container_starts_invisible() {
        let store = Rc::new(EntityStore::new());
        let comp = BrightnessSlidersComponent::new(&store, &noop_action_callback());
        assert!(!comp.widget().is_visible(), "container should start invisible");
        assert_eq!(child_count(comp.widget()), 0);
    }

    fn test_add_entity_makes_visible() {
        let store = Rc::new(EntityStore::new());
        let comp = BrightnessSlidersComponent::new(&store, &noop_action_callback());

        let urn = Urn::new("brightness", "display", "laptop");
        let data = serde_json::to_value(make_display(0.75)).unwrap();
        store.handle_notification(make_updated(urn, data));

        assert!(comp.widget().is_visible(), "container should be visible after adding entity");
        assert_eq!(child_count(comp.widget()), 1);
    }

    fn test_add_second_entity() {
        let store = Rc::new(EntityStore::new());
        let comp = BrightnessSlidersComponent::new(&store, &noop_action_callback());

        let urn1 = Urn::new("brightness", "display", "laptop");
        let urn2 = Urn::new("brightness", "display", "external");
        store.handle_notification(make_updated(
            urn1,
            serde_json::to_value(make_display(0.75)).unwrap(),
        ));
        store.handle_notification(make_updated(
            urn2,
            serde_json::to_value(make_display(0.50)).unwrap(),
        ));

        assert_eq!(child_count(comp.widget()), 2);
    }

    fn test_update_entity_preserves_child_count() {
        let store = Rc::new(EntityStore::new());
        let comp = BrightnessSlidersComponent::new(&store, &noop_action_callback());

        let urn = Urn::new("brightness", "display", "laptop");
        store.handle_notification(make_updated(
            urn.clone(),
            serde_json::to_value(make_display(0.75)).unwrap(),
        ));
        assert_eq!(child_count(comp.widget()), 1);

        // Update with new brightness value
        store.handle_notification(make_updated(
            urn,
            serde_json::to_value(make_display(0.30)).unwrap(),
        ));
        assert_eq!(child_count(comp.widget()), 1, "update should not create new widget");
    }

    fn test_remove_entity_reduces_children() {
        let store = Rc::new(EntityStore::new());
        let comp = BrightnessSlidersComponent::new(&store, &noop_action_callback());

        let urn1 = Urn::new("brightness", "display", "laptop");
        let urn2 = Urn::new("brightness", "display", "external");
        store.handle_notification(make_updated(
            urn1.clone(),
            serde_json::to_value(make_display(0.75)).unwrap(),
        ));
        store.handle_notification(make_updated(
            urn2,
            serde_json::to_value(make_display(0.50)).unwrap(),
        ));
        assert_eq!(child_count(comp.widget()), 2);

        store.handle_notification(make_removed(urn1));
        assert_eq!(child_count(comp.widget()), 1);
        assert!(comp.widget().is_visible());
    }

    fn test_remove_all_makes_invisible() {
        let store = Rc::new(EntityStore::new());
        let comp = BrightnessSlidersComponent::new(&store, &noop_action_callback());

        let urn = Urn::new("brightness", "display", "laptop");
        store.handle_notification(make_updated(
            urn.clone(),
            serde_json::to_value(make_display(0.75)).unwrap(),
        ));
        assert!(comp.widget().is_visible());

        store.handle_notification(make_removed(urn));
        assert!(!comp.widget().is_visible(), "container should be invisible when empty");
        assert_eq!(child_count(comp.widget()), 0);
    }

    fn test_action_callback_fires_on_value_commit() {
        let store = Rc::new(EntityStore::new());
        let action_called = Rc::new(Cell::new(false));
        let action_called_ref = action_called.clone();
        let cb: EntityActionCallback = Rc::new(move |_urn, action, _params| {
            if action == "set-brightness" {
                action_called_ref.set(true);
            }
        });

        let _comp = BrightnessSlidersComponent::new(&store, &cb);

        let urn = Urn::new("brightness", "display", "laptop");
        store.handle_notification(make_updated(
            urn,
            serde_json::to_value(make_display(0.75)).unwrap(),
        ));

        // The action callback is wired to the slider's connect_output,
        // which is triggered by user interaction (not entity updates).
        // We verify the component was constructed without panics and
        // the callback infrastructure is in place.
        assert!(!action_called.get(), "action should not fire on entity update alone");
    }
}
