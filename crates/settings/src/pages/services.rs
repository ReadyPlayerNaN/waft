//! Services settings page -- smart container.
//!
//! Subscribes to `EntityStore` for `user-service` entity type. On entity
//! changes, reconciles the list of service rows showing systemd user services.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use adw::prelude::*;
use waft_client::{EntityActionCallback, EntityStore};
use waft_protocol::Urn;
use waft_protocol::entity::session::{self, UserService};
use waft_ui_gtk::vdom::Component;

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
    list_box: gtk::ListBox,
    empty_state: adw::StatusPage,
    group: adw::PreferencesGroup,
}

impl ServicesPage {
    pub fn new(
        entity_store: &Rc<EntityStore>,
        action_callback: &EntityActionCallback,
        search_index: &Rc<RefCell<SearchIndex>>,
    ) -> Self {
        let root = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .spacing(24)
            .margin_top(24)
            .margin_bottom(24)
            .margin_start(12)
            .margin_end(12)
            .build();

        let empty_state = adw::StatusPage::builder()
            .icon_name("system-run-symbolic")
            .title(t("services-no-services"))
            .description(t("services-no-services-desc"))
            .visible(false)
            .build();
        root.append(&empty_state);

        let group = adw::PreferencesGroup::builder()
            .title(t("services-title"))
            .visible(false)
            .build();

        let list_box = gtk::ListBox::builder()
            .selection_mode(gtk::SelectionMode::None)
            .css_classes(["boxed-list"])
            .build();
        group.add(&list_box);
        root.append(&group);

        // Register search entries
        {
            let mut idx = search_index.borrow_mut();
            let page_title = t("settings-services");
            idx.add_section(
                "services",
                &page_title,
                &t("services-title"),
                "services-title",
                &group,
            );
        }

        let state = Rc::new(RefCell::new(ServicesPageState {
            service_rows: HashMap::new(),
            sorted_names: Vec::new(),
            list_box,
            empty_state,
            group,
        }));

        // Subscribe to user-service changes
        {
            let store = entity_store.clone();
            let state = state.clone();
            let cb = action_callback.clone();
            entity_store.subscribe_type(session::USER_SERVICE_ENTITY_TYPE, move || {
                let services: Vec<(Urn, UserService)> =
                    store.get_entities_typed(session::USER_SERVICE_ENTITY_TYPE);
                log::debug!(
                    "[services-page] Subscription triggered: {} services",
                    services.len()
                );
                Self::reconcile(&state, &services, &cb);
            });
        }

        // Trigger initial reconciliation with cached data
        {
            let store = entity_store.clone();
            let state = state.clone();
            let cb = action_callback.clone();
            gtk::glib::idle_add_local_once(move || {
                let services: Vec<(Urn, UserService)> =
                    store.get_entities_typed(session::USER_SERVICE_ENTITY_TYPE);
                if !services.is_empty() {
                    log::debug!(
                        "[services-page] Initial reconciliation: {} services",
                        services.len()
                    );
                    Self::reconcile(&state, &services, &cb);
                }
            });
        }

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
                let pos = current_names
                    .iter()
                    .position(|n| n == &service.unit)
                    .unwrap_or(0);
                state.list_box.insert(&row.widget(), pos as i32);
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
                state.list_box.remove(&row.widget());
            }
        }

        state.sorted_names = current_names;

        // Toggle empty state vs list visibility
        let has_services = !state.service_rows.is_empty();
        state.group.set_visible(has_services);
        state.empty_state.set_visible(!has_services);
    }
}
