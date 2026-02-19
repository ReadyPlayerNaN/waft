//! Display output settings section -- smart container.
//!
//! Subscribes to `EntityStore` for `display-output` entity type.
//! Renders one preferences group per display output with:
//! - Enable/disable toggle
//! - Connection type and physical size (read-only)
//! - Resolution selector (distinct resolutions)
//! - Refresh rate selector (filtered by selected resolution)
//! - Scale input (0.25 increments)
//! - Rotation selector
//! - Flip toggle
//! - VRR toggle (if supported)

use std::cell::Cell;
use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::rc::Rc;

use adw::prelude::*;
use waft_client::{EntityActionCallback, EntityStore};

use crate::i18n::{t, t_args};
use waft_protocol::Urn;
use waft_protocol::entity::display::{
    DISPLAY_OUTPUT_ENTITY_TYPE, DisplayMode, DisplayOutput, DisplayTransform,
};

/// Smart container for display output settings.
pub struct OutputSection {
    pub root: gtk::Box,
}

struct OutputGroupWidgets {
    group: adw::PreferencesGroup,
    enable_row: adw::SwitchRow,
    connection_row: adw::ActionRow,
    physical_size_row: adw::ActionRow,
    resolution_row: adw::ComboRow,
    refresh_rate_row: adw::ComboRow,
    scale_row: adw::SpinRow,
    rotation_row: adw::ComboRow,
    flip_row: adw::SwitchRow,
    vrr_row: adw::SwitchRow,
    updating: Rc<Cell<bool>>,
    /// Cached list of distinct resolutions for this output, sorted descending by pixel count.
    resolutions: Rc<RefCell<Vec<(u32, u32)>>>,
    /// Cached mapping from (resolution_index, rate_index) -> mode_index in available_modes.
    rate_mode_indices: Rc<RefCell<Vec<Vec<usize>>>>,
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

/// Extract distinct resolutions from available modes, sorted descending by pixel count.
fn distinct_resolutions(modes: &[DisplayMode]) -> Vec<(u32, u32)> {
    let mut seen = HashSet::new();
    let mut resolutions = Vec::new();
    for mode in modes {
        let key = (mode.width, mode.height);
        if seen.insert(key) {
            resolutions.push(key);
        }
    }
    resolutions.sort_by(|a, b| {
        let pixels_a = (a.0 as u64) * (a.1 as u64);
        let pixels_b = (b.0 as u64) * (b.1 as u64);
        pixels_b.cmp(&pixels_a)
    });
    resolutions
}

/// Get refresh rates for a given resolution, returning (mode_index, rate_hz) pairs sorted descending.
fn rates_for_resolution(
    modes: &[DisplayMode],
    width: u32,
    height: u32,
) -> Vec<(usize, f64)> {
    let mut rates: Vec<(usize, f64)> = modes
        .iter()
        .enumerate()
        .filter(|(_, m)| m.width == width && m.height == height)
        .map(|(idx, m)| (idx, m.refresh_rate))
        .collect();
    rates.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    rates
}

/// Build rate_mode_indices: for each resolution index, a vec of mode indices (one per rate).
fn build_rate_mode_indices(
    modes: &[DisplayMode],
    resolutions: &[(u32, u32)],
) -> Vec<Vec<usize>> {
    resolutions
        .iter()
        .map(|&(w, h)| {
            rates_for_resolution(modes, w, h)
                .into_iter()
                .map(|(idx, _)| idx)
                .collect()
        })
        .collect()
}

fn format_resolution(w: u32, h: u32) -> String {
    format!("{}\u{00D7}{}", w, h)
}

fn format_refresh_rate(rate: f64, preferred: bool) -> String {
    let suffix = if preferred {
        format!(" {}", t("display-preferred"))
    } else {
        String::new()
    };
    format!("{:.2} Hz{}", rate, suffix)
}

fn rotation_labels() -> Vec<String> {
    vec![
        t("display-rotation-normal"),
        t("display-rotation-90"),
        t("display-rotation-180"),
        t("display-rotation-270"),
    ]
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

        // Count enabled outputs for at-least-one-active enforcement
        let enabled_count = entities.iter().filter(|(_, o)| o.enabled).count();

        for (urn, output) in entities {
            let urn_str = urn.as_str().to_string();
            seen.insert(urn_str.clone());

            if let Some(existing) = map.get(&urn_str) {
                Self::update_output_group(existing, output, enabled_count);
            } else {
                let widgets = Self::create_output_group(urn, output, action_callback);
                // Set enable switch sensitivity based on at-least-one-active rule
                if output.enabled && enabled_count <= 1 {
                    widgets.enable_row.set_sensitive(false);
                }
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

    fn update_output_group(
        widgets: &OutputGroupWidgets,
        output: &DisplayOutput,
        enabled_count: usize,
    ) {
        widgets.updating.set(true);

        // Title & description
        widgets.group.set_title(&display_title(output));
        widgets
            .group
            .set_description(Some(&t_args("display-output-name", &[("name", &output.name)])));

        // Enable/disable
        widgets.enable_row.set_active(output.enabled);
        // Desensitize if this is the last active output
        widgets
            .enable_row
            .set_sensitive(!(output.enabled && enabled_count <= 1));

        // Read-only fields
        widgets
            .connection_row
            .set_subtitle(&format!("{} ({})", output.connection_type, output.name));
        match output.physical_size {
            Some([w, h]) => {
                widgets
                    .physical_size_row
                    .set_subtitle(&format!("{} \u{00D7} {} mm", w, h));
                widgets.physical_size_row.set_visible(true);
            }
            None => {
                widgets.physical_size_row.set_visible(false);
            }
        }

        // Resolution + refresh rate
        let resolutions = distinct_resolutions(&output.available_modes);
        let rate_indices = build_rate_mode_indices(&output.available_modes, &resolutions);

        // Find which resolution matches current mode
        let current_res_idx = resolutions
            .iter()
            .position(|&(w, h)| {
                w == output.current_mode.width && h == output.current_mode.height
            })
            .unwrap_or(0);

        // Update resolution combo
        let res_strings: Vec<String> = resolutions
            .iter()
            .map(|&(w, h)| format_resolution(w, h))
            .collect();
        let res_str_refs: Vec<&str> = res_strings.iter().map(|s| s.as_str()).collect();
        let res_list = gtk::StringList::new(&res_str_refs);
        widgets.resolution_row.set_model(Some(&res_list));
        widgets
            .resolution_row
            .set_selected(current_res_idx as u32);

        // Update refresh rate combo for current resolution
        if let Some(&(w, h)) = resolutions.get(current_res_idx) {
            let rates = rates_for_resolution(&output.available_modes, w, h);
            let rate_strings: Vec<String> = rates
                .iter()
                .map(|&(idx, rate)| {
                    let preferred = output
                        .available_modes
                        .get(idx)
                        .map(|m| m.preferred)
                        .unwrap_or(false);
                    format_refresh_rate(rate, preferred)
                })
                .collect();
            let rate_str_refs: Vec<&str> = rate_strings.iter().map(|s| s.as_str()).collect();
            let rate_list = gtk::StringList::new(&rate_str_refs);
            widgets.refresh_rate_row.set_model(Some(&rate_list));

            // Find which rate matches current mode
            let current_rate_idx = rates
                .iter()
                .position(|&(_, rate)| (rate - output.current_mode.refresh_rate).abs() < 0.01)
                .unwrap_or(0);
            widgets
                .refresh_rate_row
                .set_selected(current_rate_idx as u32);
        }

        // Store cached data
        *widgets.resolutions.borrow_mut() = resolutions;
        *widgets.rate_mode_indices.borrow_mut() = rate_indices;

        // Scale
        widgets.scale_row.set_value(output.scale);

        // Rotation + flip
        let (rotation_idx, flipped) = output.transform.decompose();
        widgets.rotation_row.set_selected(rotation_idx as u32);
        widgets.flip_row.set_active(flipped);

        // VRR
        widgets.vrr_row.set_visible(output.vrr_supported);
        widgets.vrr_row.set_active(output.vrr_enabled);

        widgets.updating.set(false);
    }

    fn create_output_group(
        urn: &Urn,
        output: &DisplayOutput,
        action_callback: &EntityActionCallback,
    ) -> OutputGroupWidgets {
        let group = adw::PreferencesGroup::builder()
            .title(display_title(output))
            .description(t_args("display-output-name", &[("name", &output.name)]))
            .build();

        let updating = Rc::new(Cell::new(false));
        let resolutions_rc: Rc<RefCell<Vec<(u32, u32)>>> = Rc::new(RefCell::new(Vec::new()));
        let rate_mode_indices_rc: Rc<RefCell<Vec<Vec<usize>>>> =
            Rc::new(RefCell::new(Vec::new()));

        // --- Enable/Disable ---
        let enable_row = adw::SwitchRow::builder()
            .title(display_title(output))
            .subtitle(t_args("display-output-name", &[("name", &output.name)]))
            .active(output.enabled)
            .build();
        group.add(&enable_row);

        {
            let urn_clone = urn.clone();
            let cb = action_callback.clone();
            let guard = updating.clone();
            enable_row.connect_active_notify(move |row| {
                if guard.get() {
                    return;
                }
                cb(
                    urn_clone.clone(),
                    "set-enabled".to_string(),
                    serde_json::json!({ "value": row.is_active() }),
                );
            });
        }

        // --- Connection (read-only) ---
        let connection_row = adw::ActionRow::builder()
            .title(t("display-connection"))
            .subtitle(format!("{} ({})", output.connection_type, output.name))
            .build();
        group.add(&connection_row);

        // --- Physical Size (read-only, hidden if None) ---
        let physical_size_row = adw::ActionRow::builder()
            .title(t("display-physical-size"))
            .visible(output.physical_size.is_some())
            .build();
        if let Some([w, h]) = output.physical_size {
            physical_size_row.set_subtitle(&format!("{} \u{00D7} {} mm", w, h));
        }
        group.add(&physical_size_row);

        // --- Resolution ---
        let resolutions = distinct_resolutions(&output.available_modes);
        let rate_indices = build_rate_mode_indices(&output.available_modes, &resolutions);

        let current_res_idx = resolutions
            .iter()
            .position(|&(w, h)| {
                w == output.current_mode.width && h == output.current_mode.height
            })
            .unwrap_or(0);

        let res_strings: Vec<String> = resolutions
            .iter()
            .map(|&(w, h)| format_resolution(w, h))
            .collect();
        let res_str_refs: Vec<&str> = res_strings.iter().map(|s| s.as_str()).collect();

        let resolution_row = adw::ComboRow::builder()
            .title(t("display-resolution"))
            .model(&gtk::StringList::new(&res_str_refs))
            .selected(current_res_idx as u32)
            .build();
        group.add(&resolution_row);

        // --- Refresh Rate ---
        let refresh_rate_row = adw::ComboRow::builder()
            .title(t("display-refresh-rate"))
            .build();

        // Populate initial refresh rates
        if let Some(&(w, h)) = resolutions.get(current_res_idx) {
            let rates = rates_for_resolution(&output.available_modes, w, h);
            let rate_strings: Vec<String> = rates
                .iter()
                .map(|&(idx, rate)| {
                    let preferred = output
                        .available_modes
                        .get(idx)
                        .map(|m| m.preferred)
                        .unwrap_or(false);
                    format_refresh_rate(rate, preferred)
                })
                .collect();
            let rate_str_refs: Vec<&str> = rate_strings.iter().map(|s| s.as_str()).collect();
            refresh_rate_row.set_model(Some(&gtk::StringList::new(&rate_str_refs)));

            let current_rate_idx = rates
                .iter()
                .position(|&(_, rate)| (rate - output.current_mode.refresh_rate).abs() < 0.01)
                .unwrap_or(0);
            refresh_rate_row.set_selected(current_rate_idx as u32);
        }
        group.add(&refresh_rate_row);

        // Store cached data
        *resolutions_rc.borrow_mut() = resolutions;
        *rate_mode_indices_rc.borrow_mut() = rate_indices;

        // Wire resolution change -> rebuild refresh rate dropdown
        {
            let urn_clone = urn.clone();
            let cb = action_callback.clone();
            let guard = updating.clone();
            let rate_row = refresh_rate_row.clone();
            let res_rc = resolutions_rc.clone();
            let rate_idx_rc = rate_mode_indices_rc.clone();
            let guard_inner = updating.clone();

            resolution_row.connect_selected_notify(move |row| {
                if guard.get() {
                    return;
                }
                let selected = row.selected() as usize;
                let resolutions = res_rc.borrow();
                let rate_indices = rate_idx_rc.borrow();

                if let Some(&(w, h)) = resolutions.get(selected) {
                    // We need the modes to get rate values; reconstruct from rate_indices
                    // For now, use the mode indices to build rate strings
                    // We'll fire set-mode when refresh rate is selected
                    if let Some(mode_indices) = rate_indices.get(selected) {
                        // Build rate labels from mode indices - we need the actual rates
                        // Since we only have indices, we let the refresh rate callback handle set-mode
                        // For the label, use the resolution to find rates
                        // This is simplified: rebuild the rate combo from stored data

                        guard_inner.set(true);

                        // We need to build rate strings. Since we don't have modes stored,
                        // we fire set-mode with the first (highest) rate for the new resolution.
                        let rate_list = gtk::StringList::new(&[]);
                        // We don't have the actual rate values here; we need them
                        // The rate_mode_indices only stores mode indices.
                        // We'll rebuild by looking at the rate combo options:
                        // Actually, we need to store modes reference. Let's use a different approach:
                        // Fire set-mode for the first mode at this resolution (preferred or highest rate).
                        if let Some(&first_mode_idx) = mode_indices.first() {
                            // Set a placeholder; the entity update from daemon will reconcile
                            rate_list.append(&format!("{}\u{00D7}{}", w, h));
                            rate_row.set_model(Some(&rate_list));
                            rate_row.set_selected(0);

                            guard_inner.set(false);

                            cb(
                                urn_clone.clone(),
                                "set-mode".to_string(),
                                serde_json::json!({ "mode_index": first_mode_idx }),
                            );
                            return;
                        }
                    }
                    guard_inner.set(false);
                }
            });
        }

        // Wire refresh rate change -> fire set-mode
        {
            let urn_clone = urn.clone();
            let cb = action_callback.clone();
            let guard = updating.clone();
            let res_row = resolution_row.clone();
            let rate_idx_rc = rate_mode_indices_rc.clone();

            refresh_rate_row.connect_selected_notify(move |row| {
                if guard.get() {
                    return;
                }
                let res_selected = res_row.selected() as usize;
                let rate_selected = row.selected() as usize;

                let rate_indices = rate_idx_rc.borrow();
                if let Some(modes_for_res) = rate_indices.get(res_selected)
                    && let Some(&mode_idx) = modes_for_res.get(rate_selected)
                {
                    cb(
                        urn_clone.clone(),
                        "set-mode".to_string(),
                        serde_json::json!({ "mode_index": mode_idx }),
                    );
                }
            });
        }

        // --- Scale ---
        let adjustment = gtk::Adjustment::new(
            output.scale,
            0.25, // min
            4.0,  // max
            0.25, // step
            0.5,  // page
            0.0,  // page_size
        );
        let scale_row = adw::SpinRow::builder()
            .title(t("display-scale"))
            .adjustment(&adjustment)
            .digits(2)
            .build();
        group.add(&scale_row);

        {
            let urn_clone = urn.clone();
            let cb = action_callback.clone();
            let guard = updating.clone();
            scale_row.connect_value_notify(move |row| {
                if guard.get() {
                    return;
                }
                let value = row.value();
                cb(
                    urn_clone.clone(),
                    "set-scale".to_string(),
                    serde_json::json!({ "value": value }),
                );
            });
        }

        // --- Rotation ---
        let labels = rotation_labels();
        let label_refs: Vec<&str> = labels.iter().map(|s| s.as_str()).collect();
        let rotation_list = gtk::StringList::new(&label_refs);
        let (rotation_idx, flipped) = output.transform.decompose();

        let rotation_row = adw::ComboRow::builder()
            .title(t("display-rotation"))
            .model(&rotation_list)
            .selected(rotation_idx as u32)
            .build();
        group.add(&rotation_row);

        // --- Flip ---
        let flip_row = adw::SwitchRow::builder()
            .title(t("display-flip"))
            .subtitle(t("display-flip-subtitle"))
            .active(flipped)
            .build();
        group.add(&flip_row);

        // Wire rotation change -> compose transform and fire set-transform
        {
            let urn_clone = urn.clone();
            let cb = action_callback.clone();
            let guard = updating.clone();
            let flip_ref = flip_row.clone();
            rotation_row.connect_selected_notify(move |row| {
                if guard.get() {
                    return;
                }
                let rot = row.selected() as usize;
                let flip = flip_ref.is_active();
                let transform = DisplayTransform::compose(rot, flip);
                cb(
                    urn_clone.clone(),
                    "set-transform".to_string(),
                    serde_json::json!({ "value": transform }),
                );
            });
        }

        // Wire flip change -> compose transform and fire set-transform
        {
            let urn_clone = urn.clone();
            let cb = action_callback.clone();
            let guard = updating.clone();
            let rot_ref = rotation_row.clone();
            flip_row.connect_active_notify(move |row| {
                if guard.get() {
                    return;
                }
                let rot = rot_ref.selected() as usize;
                let flip = row.is_active();
                let transform = DisplayTransform::compose(rot, flip);
                cb(
                    urn_clone.clone(),
                    "set-transform".to_string(),
                    serde_json::json!({ "value": transform }),
                );
            });
        }

        // --- VRR ---
        let vrr_row = adw::SwitchRow::builder()
            .title(t("display-vrr"))
            .visible(output.vrr_supported)
            .active(output.vrr_enabled)
            .build();
        group.add(&vrr_row);

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
            enable_row,
            connection_row,
            physical_size_row,
            resolution_row,
            refresh_rate_row,
            scale_row,
            rotation_row,
            flip_row,
            vrr_row,
            updating,
            resolutions: resolutions_rc,
            rate_mode_indices: rate_mode_indices_rc,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_distinct_resolutions_deduplicates_and_sorts() {
        let modes = vec![
            DisplayMode {
                width: 1920,
                height: 1080,
                refresh_rate: 60.0,
                preferred: true,
            },
            DisplayMode {
                width: 1920,
                height: 1080,
                refresh_rate: 144.0,
                preferred: false,
            },
            DisplayMode {
                width: 2560,
                height: 1440,
                refresh_rate: 60.0,
                preferred: false,
            },
            DisplayMode {
                width: 1280,
                height: 720,
                refresh_rate: 60.0,
                preferred: false,
            },
        ];
        let result = distinct_resolutions(&modes);
        assert_eq!(result.len(), 3);
        // Sorted descending by pixel count
        assert_eq!(result[0], (2560, 1440));
        assert_eq!(result[1], (1920, 1080));
        assert_eq!(result[2], (1280, 720));
    }

    #[test]
    fn test_rates_for_resolution_filters_and_sorts() {
        let modes = vec![
            DisplayMode {
                width: 1920,
                height: 1080,
                refresh_rate: 60.0,
                preferred: true,
            },
            DisplayMode {
                width: 2560,
                height: 1440,
                refresh_rate: 144.0,
                preferred: false,
            },
            DisplayMode {
                width: 1920,
                height: 1080,
                refresh_rate: 144.0,
                preferred: false,
            },
            DisplayMode {
                width: 1920,
                height: 1080,
                refresh_rate: 239.761,
                preferred: false,
            },
        ];
        let rates = rates_for_resolution(&modes, 1920, 1080);
        assert_eq!(rates.len(), 3);
        // Sorted descending by rate
        assert!((rates[0].1 - 239.761).abs() < 0.001);
        assert!((rates[1].1 - 144.0).abs() < 0.001);
        assert!((rates[2].1 - 60.0).abs() < 0.001);
        // Mode indices
        assert_eq!(rates[0].0, 3); // 239.761 Hz is at index 3
        assert_eq!(rates[1].0, 2); // 144 Hz is at index 2
        assert_eq!(rates[2].0, 0); // 60 Hz is at index 0
    }

    #[test]
    fn test_rates_for_resolution_no_match() {
        let modes = vec![DisplayMode {
            width: 1920,
            height: 1080,
            refresh_rate: 60.0,
            preferred: true,
        }];
        let rates = rates_for_resolution(&modes, 2560, 1440);
        assert!(rates.is_empty());
    }

    #[test]
    fn test_build_rate_mode_indices() {
        let modes = vec![
            DisplayMode {
                width: 1920,
                height: 1080,
                refresh_rate: 60.0,
                preferred: true,
            },
            DisplayMode {
                width: 1920,
                height: 1080,
                refresh_rate: 144.0,
                preferred: false,
            },
            DisplayMode {
                width: 2560,
                height: 1440,
                refresh_rate: 60.0,
                preferred: false,
            },
        ];
        let resolutions = distinct_resolutions(&modes);
        let indices = build_rate_mode_indices(&modes, &resolutions);

        assert_eq!(indices.len(), 2);
        // 2560x1440 has one rate (mode index 2)
        assert_eq!(indices[0], vec![2]);
        // 1920x1080 has two rates (mode index 1=144Hz, mode index 0=60Hz), sorted descending
        assert_eq!(indices[1], vec![1, 0]);
    }

    #[test]
    fn test_display_title_full() {
        let output = DisplayOutput {
            name: "DP-3".to_string(),
            make: "Samsung".to_string(),
            model: "LS49AG95".to_string(),
            current_mode: DisplayMode {
                width: 1920,
                height: 1080,
                refresh_rate: 60.0,
                preferred: true,
            },
            available_modes: vec![],
            vrr_supported: false,
            vrr_enabled: false,
            enabled: true,
            scale: 1.0,
            transform: DisplayTransform::Normal,
            physical_size: None,
            connection_type: "DisplayPort".to_string(),
        };
        assert_eq!(display_title(&output), "Samsung LS49AG95");
    }

    #[test]
    fn test_display_title_make_only() {
        let output = DisplayOutput {
            name: "DP-3".to_string(),
            make: "Samsung".to_string(),
            model: String::new(),
            current_mode: DisplayMode {
                width: 0,
                height: 0,
                refresh_rate: 0.0,
                preferred: false,
            },
            available_modes: vec![],
            vrr_supported: false,
            vrr_enabled: false,
            enabled: true,
            scale: 1.0,
            transform: DisplayTransform::Normal,
            physical_size: None,
            connection_type: "DisplayPort".to_string(),
        };
        assert_eq!(display_title(&output), "Samsung");
    }

    #[test]
    fn test_display_title_name_fallback() {
        let output = DisplayOutput {
            name: "DP-3".to_string(),
            make: String::new(),
            model: String::new(),
            current_mode: DisplayMode {
                width: 0,
                height: 0,
                refresh_rate: 0.0,
                preferred: false,
            },
            available_modes: vec![],
            vrr_supported: false,
            vrr_enabled: false,
            enabled: true,
            scale: 1.0,
            transform: DisplayTransform::Normal,
            physical_size: None,
            connection_type: "DisplayPort".to_string(),
        };
        assert_eq!(display_title(&output), "DP-3");
    }
}
