//! Keyboard layout header component.
//!
//! Subscribes to keyboard-layout entity type and renders the current layout
//! with a cycle button to switch between available layouts. Hides when no
//! keyboard layout entity exists.

use std::cell::RefCell;
use std::rc::Rc;

use gtk::prelude::*;

use waft_protocol::Urn;
use waft_protocol::entity;
use waft_ui_gtk::widgets::status_cycle_button::{StatusCycleButtonWidget, StatusOption};

use waft_client::{EntityActionCallback, EntityStore};

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

        let current_urn: Rc<RefCell<Option<Urn>>> = Rc::new(RefCell::new(None));

        let button = Rc::new(StatusCycleButtonWidget::new(
            "",
            "input-keyboard-symbolic",
            &[],
            {
                let cb = action_callback.clone();
                let urn_ref = current_urn.clone();
                Rc::new(move |layout_id: String| {
                    if let Some(urn) = urn_ref.borrow().as_ref() {
                        cb(
                            urn.clone(),
                            "cycle".to_string(),
                            serde_json::json!(layout_id),
                        );
                    } else {
                        log::warn!(
                            "[keyboard-layout] action triggered before entity URN is known, ignoring"
                        );
                    }
                })
            },
        ));
        container.append(&button.widget());

        let store_ref = store.clone();
        let button_ref = button.clone();
        let container_ref = container.clone();
        let urn_for_sub = current_urn;
        store.subscribe_type(entity::keyboard::ENTITY_TYPE, move || {
            let entities = store_ref.get_entities_typed::<entity::keyboard::KeyboardLayout>(
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
