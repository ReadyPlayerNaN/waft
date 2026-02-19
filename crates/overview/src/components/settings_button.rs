//! Settings button header component.
//!
//! Subscribes to the app entity type and shows a gear icon button when the
//! waft-settings app entity is present. Hides when no app entity exists.

use std::cell::RefCell;
use std::rc::Rc;

use gtk::prelude::*;

use waft_protocol::Urn;
use waft_protocol::entity;
use waft_ui_gtk::widgets::icon::IconWidget;

use waft_client::{EntityActionCallback, EntityStore};

use crate::i18n;

/// Displays a settings gear button that opens waft-settings on click.
///
/// Automatically hides when no app entity exists.
pub struct SettingsButtonComponent {
    container: gtk::Box,
}

impl SettingsButtonComponent {
    pub fn new(store: &Rc<EntityStore>, action_callback: &EntityActionCallback) -> Self {
        let container = gtk::Box::builder()
            .orientation(gtk::Orientation::Horizontal)
            .visible(false)
            .build();

        let current_urn: Rc<RefCell<Option<Urn>>> = Rc::new(RefCell::new(None));

        let label = i18n::t("settings-button");
        let icon = IconWidget::from_name("emblem-system-symbolic", 16);

        let button_content = gtk::Box::new(gtk::Orientation::Horizontal, 0);
        button_content.append(icon.widget());

        let button = gtk::Button::builder()
            .child(&button_content)
            .tooltip_text(&label)
            .css_classes(["flat", "circular"])
            .build();
        button.update_property(&[gtk::accessible::Property::Label(&label)]);

        {
            let cb = action_callback.clone();
            let urn_ref = current_urn.clone();
            button.connect_clicked(move |_| {
                if let Some(urn) = urn_ref.borrow().as_ref() {
                    cb(
                        urn.clone(),
                        "open".to_string(),
                        serde_json::Value::Null,
                    );
                } else {
                    log::warn!(
                        "[settings-button] action triggered before entity URN is known, ignoring"
                    );
                }
            });
        }

        container.append(&button);

        let store_ref = store.clone();
        let container_ref = container.clone();
        let urn_for_sub = current_urn;

        let reconcile = {
            let store_ref = store_ref.clone();
            let container_ref = container_ref.clone();
            let urn_for_sub = urn_for_sub.clone();
            move || {
                let entities =
                    store_ref.get_entities_typed::<entity::app::App>(entity::app::ENTITY_TYPE);
                match entities.first() {
                    Some((urn, _app)) => {
                        *urn_for_sub.borrow_mut() = Some(urn.clone());
                        container_ref.set_visible(true);
                    }
                    None => {
                        *urn_for_sub.borrow_mut() = None;
                        container_ref.set_visible(false);
                    }
                }
            }
        };

        store.subscribe_type(entity::app::ENTITY_TYPE, reconcile.clone());

        // Initial reconciliation: entities may have arrived before subscription
        glib::idle_add_local_once(reconcile);

        Self { container }
    }

    pub fn widget(&self) -> gtk::Widget {
        self.container.clone().upcast()
    }
}
