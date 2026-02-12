//! Keyboard layout header component.
//!
//! Subscribes to keyboard-layout entity type and renders the current layout
//! with a cycle button to switch between available layouts. Hides when no
//! keyboard layout entity exists.

use std::cell::RefCell;
use std::rc::Rc;

use gtk::prelude::*;

use waft_ipc::widget::{Action, ActionParams, StatusOption};
use waft_protocol::entity;
use waft_protocol::Urn;
use waft_ui_gtk::renderer::ActionCallback;
use waft_ui_gtk::widgets::status_cycle_button::StatusCycleButtonWidget;

use crate::entity_store::{EntityActionCallback, EntityStore};

/// Displays the current keyboard layout and cycles to the next on click.
///
/// Automatically hides when no keyboard layout entity exists.
pub struct KeyboardLayoutComponent {
    container: gtk::Box,
    _button: Rc<StatusCycleButtonWidget>,
}

impl KeyboardLayoutComponent {
    pub fn new(store: &Rc<EntityStore>, action_callback: &EntityActionCallback) -> Self {
        let container = gtk::Box::builder()
            .orientation(gtk::Orientation::Horizontal)
            .visible(false)
            .build();

        // Shared URN reference updated by the subscription callback.
        // The ActionCallback closure reads this to route actions to the
        // correct entity URN once it becomes known.
        let current_urn: Rc<RefCell<Option<Urn>>> = Rc::new(RefCell::new(None));

        // Build an ActionCallback that bridges to EntityActionCallback.
        // The StatusCycleButtonWidget passes (widget_id, Action) but we
        // ignore widget_id and use the shared current_urn instead, since
        // the widget_id is captured at construction time and cannot be
        // updated when the entity URN becomes known.
        let entity_cb = action_callback.clone();
        let urn_for_action = current_urn.clone();
        let action_cb: ActionCallback = Rc::new(move |_widget_id: String, action: Action| {
            let urn = match urn_for_action.borrow().clone() {
                Some(urn) => urn,
                None => {
                    log::warn!(
                        "[keyboard-layout] action triggered before entity URN is known, ignoring"
                    );
                    return;
                }
            };
            let params = action_params_to_value(&action.params);
            entity_cb(urn, action.id, params);
        });

        // The widget_id placeholder is unused because we route via current_urn.
        let button = Rc::new(StatusCycleButtonWidget::new(
            "",
            "input-keyboard-symbolic",
            &[],
            &action_cb,
            &Action {
                id: "cycle".to_string(),
                params: ActionParams::None,
            },
            "keyboard-layout",
        ));
        container.append(&button.widget());

        let store_ref = store.clone();
        let button_ref = button.clone();
        let container_ref = container.clone();
        let urn_for_sub = current_urn;
        store.subscribe_type(entity::keyboard::ENTITY_TYPE, move || {
            let entities = store_ref
                .get_entities_typed::<entity::keyboard::KeyboardLayout>(
                    entity::keyboard::ENTITY_TYPE,
                );
            match entities.first() {
                Some((urn, layout)) => {
                    *urn_for_sub.borrow_mut() = Some(urn.clone());
                    let options: Vec<StatusOption> = layout
                        .available
                        .iter()
                        .map(|name| StatusOption {
                            id: name.clone(),
                            label: name.clone(),
                        })
                        .collect();
                    button_ref.set_value(&layout.current);
                    button_ref.set_options(&options);
                    container_ref.set_visible(true);
                }
                None => {
                    *urn_for_sub.borrow_mut() = None;
                    container_ref.set_visible(false);
                }
            }
        });

        Self {
            container,
            _button: button,
        }
    }

    pub fn widget(&self) -> gtk::Widget {
        self.container.clone().upcast()
    }
}

/// Convert ActionParams to a serde_json::Value for the entity action protocol.
fn action_params_to_value(params: &ActionParams) -> serde_json::Value {
    match params {
        ActionParams::None => serde_json::Value::Null,
        ActionParams::Value(v) => serde_json::json!(*v),
        ActionParams::String(s) => serde_json::json!(s),
        ActionParams::Map(m) => serde_json::json!(m),
    }
}
