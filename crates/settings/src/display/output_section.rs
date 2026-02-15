//! Display output settings section -- smart container.
//!
//! Subscribes to `EntityStore` for `display-output` entity type.
//! Renders one preferences group per display output with resolution
//! dropdown and optional VRR toggle.

use std::cell::Cell;
use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::rc::Rc;

use adw::prelude::*;
use waft_client::{EntityActionCallback, EntityStore};
use waft_protocol::Urn;
use waft_protocol::entity::display::{DISPLAY_OUTPUT_ENTITY_TYPE, DisplayMode, DisplayOutput};

/// Smart container for display output resolution settings.
pub struct OutputSection {
    pub root: gtk::Box,
}

struct OutputGroupWidgets {
    group: adw::PreferencesGroup,
    mode_dropdown: gtk::DropDown,
    vrr_row: adw::SwitchRow,
    updating: Rc<Cell<bool>>,
}

fn format_mode(mode: &DisplayMode) -> String {
    let hz = format!("{:.2}", mode.refresh_rate);
    let preferred = if mode.preferred { " (Preferred)" } else { "" };
    format!("{}\u{00D7}{} @ {} Hz{}", mode.width, mode.height, hz, preferred)
}

fn display_title(output: &DisplayOutput) -> String {
    if output.make.is_empty() && output.model.is_empty() {
        output.name.clone()
    } else if output.make.is_empty() {
        output.model.clone()
    } else if output.model.is_empty() {
        output.make.clone()
    } else {
        format!("{} {}", output.make, output.model)
    }
}

impl OutputSection {
    pub fn new(entity_store: &Rc<EntityStore>, action_callback: &EntityActionCallback) -> Self {
        let root = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .spacing(24)
            .visible(false)
            .build();

        let outputs: Rc<RefCell<HashMap<String, OutputGroupWidgets>>> =
            Rc::new(RefCell::new(HashMap::new()));

        // Subscribe to display-output entities
        {
            let store = entity_store.clone();
            let cb = action_callback.clone();
            let root_ref = root.clone();
            let outputs_ref = outputs.clone();

            entity_store.subscribe_type(DISPLAY_OUTPUT_ENTITY_TYPE, move || {
                let entities: Vec<(Urn, DisplayOutput)> =
                    store.get_entities_typed(DISPLAY_OUTPUT_ENTITY_TYPE);
                Self::reconcile(&outputs_ref, &root_ref, &entities, &cb);
            });
        }

        // Initial reconciliation for cached entities
        {
            let store = entity_store.clone();
            let cb = action_callback.clone();
            let root_ref = root.clone();
            let outputs_ref = outputs;

            gtk::glib::idle_add_local_once(move || {
                let entities: Vec<(Urn, DisplayOutput)> =
                    store.get_entities_typed(DISPLAY_OUTPUT_ENTITY_TYPE);
                if !entities.is_empty() {
                    log::debug!(
                        "[output-section] Initial reconciliation: {} outputs",
                        entities.len()
                    );
                    Self::reconcile(&outputs_ref, &root_ref, &entities, &cb);
                }
            });
        }

        Self { root }
    }

    fn reconcile(
        outputs_map: &Rc<RefCell<HashMap<String, OutputGroupWidgets>>>,
        root: &gtk::Box,
        entities: &[(Urn, DisplayOutput)],
        action_callback: &EntityActionCallback,
    ) {
        let mut map = outputs_map.borrow_mut();
        let mut seen = HashSet::new();

        for (urn, output) in entities {
            let urn_str = urn.as_str().to_string();
            seen.insert(urn_str.clone());

            if let Some(existing) = map.get(&urn_str) {
                existing.updating.set(true);

                existing.group.set_title(&display_title(output));
                existing
                    .group
                    .set_description(Some(&format!("Output: {}", output.name)));

                let current_idx = output
                    .available_modes
                    .iter()
                    .position(|m| m == &output.current_mode)
                    .unwrap_or(0);

                let string_list = gtk::StringList::new(&[]);
                for mode in &output.available_modes {
                    string_list.append(&format_mode(mode));
                }
                existing.mode_dropdown.set_model(Some(&string_list));
                existing.mode_dropdown.set_selected(current_idx as u32);

                existing.vrr_row.set_visible(output.vrr_supported);
                existing.vrr_row.set_active(output.vrr_enabled);

                existing.updating.set(false);
            } else {
                let widgets = Self::create_output_group(urn, output, action_callback);
                root.append(&widgets.group);
                map.insert(urn_str, widgets);
            }
        }

        // Remove stale groups
        let to_remove: Vec<String> = map
            .keys()
            .filter(|k| !seen.contains(*k))
            .cloned()
            .collect();
        for key in to_remove {
            if let Some(widgets) = map.remove(&key) {
                root.remove(&widgets.group);
            }
        }

        root.set_visible(!map.is_empty());
    }

    fn create_output_group(
        urn: &Urn,
        output: &DisplayOutput,
        action_callback: &EntityActionCallback,
    ) -> OutputGroupWidgets {
        let group = adw::PreferencesGroup::builder()
            .title(&display_title(output))
            .description(format!("Output: {}", output.name))
            .build();

        let current_idx = output
            .available_modes
            .iter()
            .position(|m| m == &output.current_mode)
            .unwrap_or(0);

        let string_list = gtk::StringList::new(&[]);
        for mode in &output.available_modes {
            string_list.append(&format_mode(mode));
        }

        let mode_dropdown = gtk::DropDown::builder()
            .model(&string_list)
            .selected(current_idx as u32)
            .build();

        let mode_row = adw::ActionRow::builder().title("Resolution").build();
        mode_row.add_suffix(&mode_dropdown);
        group.add(&mode_row);

        let updating = Rc::new(Cell::new(false));

        // Wire mode dropdown callback
        {
            let urn_clone = urn.clone();
            let cb = action_callback.clone();
            let guard = updating.clone();
            mode_dropdown.connect_selected_notify(move |dropdown| {
                if guard.get() {
                    return;
                }
                let idx = dropdown.selected() as usize;
                cb(
                    urn_clone.clone(),
                    "set-mode".to_string(),
                    serde_json::json!({ "mode_index": idx }),
                );
            });
        }

        // Create VRR switch
        let vrr_row = adw::SwitchRow::builder()
            .title("Variable Refresh Rate")
            .visible(output.vrr_supported)
            .active(output.vrr_enabled)
            .build();
        group.add(&vrr_row);

        // Wire VRR callback
        {
            let urn_clone = urn.clone();
            let cb = action_callback.clone();
            let guard = updating.clone();
            vrr_row.connect_active_notify(move |_row| {
                if guard.get() {
                    return;
                }
                cb(
                    urn_clone.clone(),
                    "toggle-vrr".to_string(),
                    serde_json::Value::Null,
                );
            });
        }

        OutputGroupWidgets {
            group,
            mode_dropdown,
            vrr_row,
            updating,
        }
    }
}
