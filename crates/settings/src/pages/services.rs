//! Services settings page -- smart container.
//!
//! Subscribes to `EntityStore` for `user-service` entity type. On entity
//! changes, reconciles the list of service rows showing systemd user services.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use waft_client::{EntityActionCallback, EntityStore};
use waft_protocol::Urn;
use waft_protocol::entity::session::{self, UserService};
use waft_ui_gtk::vdom::Component;

use crate::entity_list_group::EntityListGroup;
use crate::i18n::t;
use crate::search_index::SearchIndex;
use crate::services::service_row::{ServiceRow, ServiceRowOutput, ServiceRowProps};

/// Smart container for the Services settings page.
pub struct ServicesPage {
    pub root: gtk::Box,
}

/// Internal mutable state for the Services page.
struct ServicesPageState {
    service_rows: HashMap<String, (ServiceRow, Urn)>,
    sorted_names: Vec<String>,
    list_group: EntityListGroup,
}

impl ServicesPage {
    pub fn new(
        entity_store: &Rc<EntityStore>,
        action_callback: &EntityActionCallback,
        search_index: &Rc<RefCell<SearchIndex>>,
    ) -> Self {
        let root = crate::page_layout::page_root();

        let list_group = EntityListGroup::new(
            &root,
            "system-run-symbolic",
            &t("services-no-services"),
            &t("services-no-services-desc"),
            &t("services-title"),
        );

        // Register search entries
        {
            let mut idx = search_index.borrow_mut();
            let page_title = t("settings-services");
            idx.add_section(
                "services",
                &page_title,
                &t("services-title"),
                "services-title",
                &list_group.group,
            );
        }

        let state = Rc::new(RefCell::new(ServicesPageState {
            service_rows: HashMap::new(),
            sorted_names: Vec::new(),
            list_group,
        }));

        // Subscribe to user-service changes (future updates + initial reconciliation)
        crate::subscription::subscribe_entities::<UserService, _>(
            entity_store,
            session::USER_SERVICE_ENTITY_TYPE,
            {
                let state = state.clone();
                let cb = action_callback.clone();
                move |services| {
                    log::debug!(
                        "[services-page] Reconciling: {} services",
                        services.len()
                    );
                    Self::reconcile(&state, &services, &cb);
                }
            },
        );

        Self { root }
    }

    /// Reconcile the service row list with current entity data.
    fn reconcile(
        state: &Rc<RefCell<ServicesPageState>>,
        services: &[(Urn, UserService)],
        action_callback: &EntityActionCallback,
    ) {
        let mut state = state.borrow_mut();

        // Build sorted list of unit names for stable alphabetical ordering
        let mut current_names: Vec<String> = services.iter().map(|(_, s)| s.unit.clone()).collect();
        current_names.sort();
        current_names.dedup();

        let mut seen = std::collections::HashSet::new();

        for (urn, service) in services {
            seen.insert(service.unit.clone());

            let props = ServiceRowProps {
                unit: service.unit.clone(),
                description: service.description.clone(),
                active_state: service.active_state.clone(),
                enabled: service.enabled,
                sub_state: service.sub_state.clone(),
            };

            if let Some((existing, _)) = state.service_rows.get(&service.unit) {
                existing.update(&props);
            } else {
                let row = ServiceRow::build(&props);

                // Wire output events
                let cb = action_callback.clone();
                let row_urn = urn.clone();
                row.connect_output(move |output| {
                    let action = match output {
                        ServiceRowOutput::Start => "start",
                        ServiceRowOutput::Stop => "stop",
                        ServiceRowOutput::Enable => "enable",
                        ServiceRowOutput::Disable => "disable",
                    };
                    cb(
                        row_urn.clone(),
                        action.to_string(),
                        serde_json::Value::Null,
                    );
                });

                // Insert in sorted position
                state.list_group.insert_sorted(&row.widget(), &service.unit, &current_names);
                state
                    .service_rows
                    .insert(service.unit.clone(), (row, urn.clone()));
            }
        }

        // Remove rows no longer present
        let to_remove: Vec<String> = state
            .service_rows
            .keys()
            .filter(|k| !seen.contains(*k))
            .cloned()
            .collect();

        for key in to_remove {
            if let Some((row, _)) = state.service_rows.remove(&key) {
                state.list_group.list_box.remove(&row.widget());
            }
        }

        state.sorted_names = current_names;
        state.list_group.toggle_visibility(!state.service_rows.is_empty());
    }
}
