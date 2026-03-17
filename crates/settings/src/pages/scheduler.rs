//! Scheduler settings page -- smart container.
//!
//! Subscribes to `EntityStore` for `user-timer` entity type. On entity
//! changes, reconciles the list of timer rows showing systemd user timers.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use adw::prelude::*;
use waft_client::{EntityActionCallback, EntityStore};
use waft_protocol::Urn;
use waft_protocol::entity::session::{self, UserTimer};
use waft_ui_gtk::vdom::Component;

use crate::entity_list_group::EntityListGroup;
use crate::i18n::t;
use crate::scheduler::timer_dialog::TimerDialog;
use crate::scheduler::timer_row::{TimerRow, TimerRowOutput, TimerRowProps};
use crate::search_index::SearchIndex;

/// Smart container for the Scheduler settings page.
pub struct SchedulerPage {
    pub root: gtk::Box,
}

/// Internal mutable state for the Scheduler page.
struct SchedulerPageState {
    timer_rows: HashMap<String, (TimerRow, Urn, UserTimer)>,
    sorted_names: Vec<String>,
    list_group: EntityListGroup,
}

impl SchedulerPage {
    pub fn new(
        entity_store: &Rc<EntityStore>,
        action_callback: &EntityActionCallback,
        search_index: &Rc<RefCell<SearchIndex>>,
    ) -> Self {
        let root = crate::page_layout::page_root();

        // -- Add timer button section --
        let add_group = adw::PreferencesGroup::new();
        let add_button = gtk::Button::builder()
            .label(t("scheduler-add-timer"))
            .css_classes(["suggested-action"])
            .build();
        add_group.add(&add_button);
        root.append(&add_group);

        // -- Empty state + timer list --
        let list_group = EntityListGroup::new(
            &root,
            "preferences-system-time-symbolic",
            &t("scheduler-no-timers"),
            &t("scheduler-no-timers-desc"),
            &t("scheduler-title"),
        );

        // Register search entries
        {
            let mut idx = search_index.borrow_mut();
            let page_title = t("settings-scheduled-tasks");
            idx.add_section(
                "scheduled-tasks",
                &page_title,
                &t("scheduler-title"),
                "scheduler-title",
                &list_group.group,
            );
        }

        let state = Rc::new(RefCell::new(SchedulerPageState {
            timer_rows: HashMap::new(),
            sorted_names: Vec::new(),
            list_group,
        }));

        // Wire "Add Timer" button
        {
            let cb = action_callback.clone();
            let root_ref = root.clone();
            add_button.connect_clicked(move |_| {
                let dialog = TimerDialog::new(None);
                let cb_inner = cb.clone();
                dialog.connect_confirmed(move |timer| {
                    let value = serde_json::to_value(&timer).unwrap_or(serde_json::Value::Null);
                    let urn = Urn::new("systemd", "user-timer", &timer.name);
                    cb_inner(urn, "create".to_string(), value);
                });
                dialog.present(&root_ref);
            });
        }

        // Show an alert dialog when an action on a user-timer fails.
        {
            let root_ref = root.clone();
            entity_store.on_action_error(move |_action_id, error| {
                let dialog = adw::AlertDialog::builder()
                    .heading(t("scheduler-action-failed"))
                    .body(error)
                    .close_response("ok")
                    .default_response("ok")
                    .build();
                dialog.add_response("ok", &t("scheduler-ok"));
                dialog.present(Some(&root_ref));
            });
        }

        // Subscribe to user-timer changes
        crate::subscription::subscribe_entities::<UserTimer, _>(
            entity_store,
            session::USER_TIMER_ENTITY_TYPE,
            {
                let state = state.clone();
                let cb = action_callback.clone();
                let root_ref = root.clone();
                move |timers| {
                    log::debug!(
                        "[scheduler-page] Reconciling: {} timers",
                        timers.len()
                    );
                    Self::reconcile(&state, &timers, &cb, &root_ref);
                }
            },
        );

        Self { root }
    }

    /// Reconcile the timer row list with current entity data.
    fn reconcile(
        state: &Rc<RefCell<SchedulerPageState>>,
        timers: &[(Urn, UserTimer)],
        action_callback: &EntityActionCallback,
        root: &gtk::Box,
    ) {
        let mut state = state.borrow_mut();

        // Build sorted list of timer names for stable alphabetical ordering
        let mut current_names: Vec<String> = timers.iter().map(|(_, t)| t.name.clone()).collect();
        current_names.sort();
        current_names.dedup();

        let mut seen = std::collections::HashSet::new();

        for (urn, timer) in timers {
            seen.insert(timer.name.clone());

            let props = TimerRowProps {
                name: timer.name.clone(),
                description: timer.description.clone(),
                schedule: timer.schedule.clone(),
                enabled: timer.enabled,
                active: timer.active,
            };

            if let Some((existing, _, stored_timer)) = state.timer_rows.get_mut(&timer.name) {
                existing.update(&props);
                *stored_timer = timer.clone();
            } else {
                let row = TimerRow::build(&props);

                // Wire output events
                let cb = action_callback.clone();
                let row_urn = urn.clone();
                let row_timer = timer.clone();
                let root_ref = root.clone();
                let cb_for_edit = action_callback.clone();
                let urn_for_edit = urn.clone();
                row.connect_output(move |output| {
                    match output {
                        TimerRowOutput::Enable => {
                            cb(
                                row_urn.clone(),
                                "enable".to_string(),
                                serde_json::Value::Null,
                            );
                        }
                        TimerRowOutput::Disable => {
                            cb(
                                row_urn.clone(),
                                "disable".to_string(),
                                serde_json::Value::Null,
                            );
                        }
                        TimerRowOutput::RunNow => {
                            cb(
                                row_urn.clone(),
                                "start".to_string(),
                                serde_json::Value::Null,
                            );
                        }
                        TimerRowOutput::Edit => {
                            let dialog = TimerDialog::new(Some(&row_timer));
                            let cb_inner = cb_for_edit.clone();
                            let urn_inner = urn_for_edit.clone();
                            dialog.connect_confirmed(move |updated_timer| {
                                let value = serde_json::to_value(&updated_timer)
                                    .unwrap_or(serde_json::Value::Null);
                                cb_inner(urn_inner.clone(), "update".to_string(), value);
                            });
                            dialog.present(&root_ref);
                        }
                        TimerRowOutput::Delete => {
                            let cb_inner = cb.clone();
                            let urn_inner = row_urn.clone();
                            let root_inner = root_ref.clone();
                            let confirm = adw::AlertDialog::builder()
                                .heading(t("scheduler-delete-timer"))
                                .body(&row_timer.name)
                                .close_response("cancel")
                                .default_response("cancel")
                                .build();
                            confirm.add_response("cancel", &t("notif-cancel"));
                            confirm.add_response("delete", &t("scheduler-delete-timer"));
                            confirm.set_response_appearance(
                                "delete",
                                adw::ResponseAppearance::Destructive,
                            );
                            confirm.connect_response(None, move |_, response| {
                                if response == "delete" {
                                    cb_inner(
                                        urn_inner.clone(),
                                        "delete".to_string(),
                                        serde_json::Value::Null,
                                    );
                                }
                            });
                            confirm.present(Some(&root_inner));
                        }
                    }
                });

                // Insert in sorted position
                state.list_group.insert_sorted(&row.widget(), &timer.name, &current_names);
                state
                    .timer_rows
                    .insert(timer.name.clone(), (row, urn.clone(), timer.clone()));
            }
        }

        // Remove rows no longer present
        let to_remove: Vec<String> = state
            .timer_rows
            .keys()
            .filter(|k| !seen.contains(*k))
            .cloned()
            .collect();

        for key in to_remove {
            if let Some((row, _, _)) = state.timer_rows.remove(&key) {
                state.list_group.list_box.remove(&row.widget());
            }
        }

        state.sorted_names = current_names;
        state.list_group.toggle_visibility(!state.timer_rows.is_empty());
    }
}
