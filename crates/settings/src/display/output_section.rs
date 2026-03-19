//! Display output widget helpers.
//!
//! Types and functions for display output controls:
//! - Enable/disable toggle
//! - Connection type and physical size (read-only)
//! - Resolution selector (distinct resolutions)
//! - Refresh rate selector (filtered by selected resolution)
//! - Scale input (SpinRow with 0.05 step + slider snapping to 0.5 steps)
//! - Rotation selector
//! - Flip toggle
//! - VRR toggle (if supported)
//!
//! All mutable changes are buffered in `PendingOutputChanges` per output URN.
//! Changes are only sent to the daemon when the user clicks Apply.

use std::cell::Cell;
use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::rc::Rc;

use adw::prelude::*;

use crate::i18n::t;
use crate::i18n::t_args;
use waft_protocol::Urn;
use waft_protocol::entity::display::{
    DisplayMode, DisplayOutput, DisplayTransform,
};

/// Pending (uncommitted) display output changes for one URN.
#[derive(Default)]
pub(crate) struct PendingOutputChanges {
    /// New mode index to apply (from resolution or refresh rate change).
    pub(crate) mode_index: Option<usize>,
    /// New scale value.
    pub(crate) scale: Option<f64>,
    /// New transform (combines rotation + flip).
    pub(crate) transform: Option<DisplayTransform>,
    /// New VRR state.
    pub(crate) vrr: Option<bool>,
    /// New enabled state.
    pub(crate) enabled: Option<bool>,
}

impl PendingOutputChanges {
    pub(crate) fn dirty(&self) -> bool {
        self.mode_index.is_some()
            || self.scale.is_some()
            || self.transform.is_some()
            || self.vrr.is_some()
            || self.enabled.is_some()
    }
}

pub(crate) struct OutputGroupWidgets {
    pub(crate) group: adw::PreferencesGroup,
    pub(crate) enable_row: adw::SwitchRow,
    pub(crate) connection_row: adw::ActionRow,
    pub(crate) physical_size_row: adw::ActionRow,
    pub(crate) resolution_row: adw::ComboRow,
    pub(crate) refresh_rate_row: adw::ComboRow,
    pub(crate) scale_row: adw::SpinRow,
    #[allow(dead_code)] // Kept for ownership; updates via shared adjustment
    pub(crate) scale_slider: gtk::Scale,
    #[allow(dead_code)] // Kept for ownership; shared between SpinRow and Scale
    pub(crate) scale_adjustment: gtk::Adjustment,
    pub(crate) rotation_row: adw::ComboRow,
    pub(crate) flip_row: adw::SwitchRow,
    pub(crate) vrr_row: adw::SwitchRow,
    pub(crate) updating: Rc<Cell<bool>>,
    /// Cached list of distinct resolutions for this output, sorted descending by pixel count.
    pub(crate) resolutions: Rc<RefCell<Vec<(u32, u32)>>>,
    /// Cached mapping from (resolution_index, rate_index) -> mode_index in available_modes.
    pub(crate) rate_mode_indices: Rc<RefCell<Vec<Vec<usize>>>>,
}

pub(crate) fn display_title(output: &DisplayOutput) -> String {
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
pub(crate) fn distinct_resolutions(modes: &[DisplayMode]) -> Vec<(u32, u32)> {
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
pub(crate) fn rates_for_resolution(
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
pub(crate) fn build_rate_mode_indices(
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

pub(crate) fn format_resolution(w: u32, h: u32) -> String {
    format!("{}\u{00D7}{}", w, h)
}

pub(crate) fn format_refresh_rate(rate: f64, preferred: bool) -> String {
    let suffix = if preferred {
        format!(" {}", t("display-preferred"))
    } else {
        String::new()
    };
    format!("{:.2} Hz{}", rate, suffix)
}

pub(crate) fn rotation_labels() -> Vec<String> {
    vec![
        t("display-rotation-normal"),
        t("display-rotation-90"),
        t("display-rotation-180"),
        t("display-rotation-270"),
    ]
}

/// Check if any output has pending changes.
pub(crate) fn any_dirty(pending: &HashMap<String, PendingOutputChanges>) -> bool {
    pending.values().any(|p| p.dirty())
}

/// Update an existing output group's widgets from new entity data.
pub(crate) fn update_output_group(
    widgets: &OutputGroupWidgets,
    output: &DisplayOutput,
    enabled_count: usize,
    pending: Option<&PendingOutputChanges>,
) {
    widgets.updating.set(true);

    // Title & description (always update -- read-only)
    widgets.group.set_title(&display_title(output));

    // Enable/disable -- skip if pending
    if pending.and_then(|p| p.enabled).is_none() {
        widgets.enable_row.set_active(output.enabled);
    }
    // Desensitize if this is the last active output
    let effective_enabled = pending
        .and_then(|p| p.enabled)
        .unwrap_or(output.enabled);
    widgets
        .enable_row
        .set_sensitive(!(effective_enabled && enabled_count <= 1));

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

    // Resolution + refresh rate -- skip if pending mode_index
    let resolutions = distinct_resolutions(&output.available_modes);
    let rate_indices = build_rate_mode_indices(&output.available_modes, &resolutions);

    if pending.and_then(|p| p.mode_index).is_none() {
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
    }

    // Store cached data (always update -- needed for local resolution/rate switching)
    *widgets.resolutions.borrow_mut() = resolutions;
    *widgets.rate_mode_indices.borrow_mut() = rate_indices;

    // Scale -- skip if pending
    if pending.and_then(|p| p.scale).is_none() {
        widgets.scale_row.set_value(output.scale);
    }

    // Rotation + flip -- skip if pending transform
    if pending.and_then(|p| p.transform).is_none() {
        let (rotation_idx, flipped) = output.transform.decompose();
        widgets.rotation_row.set_selected(rotation_idx as u32);
        widgets.flip_row.set_active(flipped);
    }

    // VRR
    widgets.vrr_row.set_visible(output.vrr_supported);
    if pending.and_then(|p| p.vrr).is_none() {
        widgets.vrr_row.set_active(output.vrr_enabled);
    }

    widgets.updating.set(false);
}

/// Create output control rows and add them to an existing preferences group.
/// Returns the OutputGroupWidgets with the provided group.
pub(crate) fn create_output_rows(
    group: &adw::PreferencesGroup,
    urn: &Urn,
    output: &DisplayOutput,
    pending: &Rc<RefCell<HashMap<String, PendingOutputChanges>>>,
    apply_button: &gtk::Button,
    reset_button: &gtk::Button,
) -> OutputGroupWidgets {
    let group = group.clone();

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
        let urn_str = urn.as_str().to_string();
        let pending_ref = pending.clone();
        let guard = updating.clone();
        let apply_btn = apply_button.clone();
        let reset_btn = reset_button.clone();
        enable_row.connect_active_notify(move |row| {
            if guard.get() {
                return;
            }
            let mut map = pending_ref.borrow_mut();
            let entry = map.entry(urn_str.clone()).or_default();
            entry.enabled = Some(row.is_active());
            drop(map);
            apply_btn.set_sensitive(true);
            reset_btn.set_sensitive(true);
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

    // Wire resolution change -> rebuild refresh rate dropdown locally + set pending mode
    {
        let urn_str = urn.as_str().to_string();
        let pending_ref = pending.clone();
        let guard = updating.clone();
        let rate_row = refresh_rate_row.clone();
        let res_rc = resolutions_rc.clone();
        let rate_idx_rc = rate_mode_indices_rc.clone();
        let guard_inner = updating.clone();
        let apply_btn = apply_button.clone();
        let reset_btn = reset_button.clone();

        resolution_row.connect_selected_notify(move |row| {
            if guard.get() {
                return;
            }
            let selected = row.selected() as usize;
            let resolutions = res_rc.borrow();
            let rate_indices = rate_idx_rc.borrow();

            if let Some(&(w, h)) = resolutions.get(selected) {
                if let Some(mode_indices) = rate_indices.get(selected) {
                    guard_inner.set(true);

                    let rate_list = gtk::StringList::new(&[]);
                    if let Some(&first_mode_idx) = mode_indices.first() {
                        rate_list.append(&format!("{}\u{00D7}{}", w, h));
                        rate_row.set_model(Some(&rate_list));
                        rate_row.set_selected(0);

                        guard_inner.set(false);

                        let mut map = pending_ref.borrow_mut();
                        let entry = map.entry(urn_str.clone()).or_default();
                        entry.mode_index = Some(first_mode_idx);
                        drop(map);
                        apply_btn.set_sensitive(true);
                        reset_btn.set_sensitive(true);
                        return;
                    }
                }
                guard_inner.set(false);
            }
        });
    }

    // Wire refresh rate change -> set pending mode
    {
        let urn_str = urn.as_str().to_string();
        let pending_ref = pending.clone();
        let guard = updating.clone();
        let res_row = resolution_row.clone();
        let rate_idx_rc = rate_mode_indices_rc.clone();
        let apply_btn = apply_button.clone();
        let reset_btn = reset_button.clone();

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
                let mut map = pending_ref.borrow_mut();
                let entry = map.entry(urn_str.clone()).or_default();
                entry.mode_index = Some(mode_idx);
                drop(map);
                apply_btn.set_sensitive(true);
                reset_btn.set_sensitive(true);
            }
        });
    }

    // --- Scale ---
    let scale_adjustment = gtk::Adjustment::new(
        output.scale,
        0.5,  // min
        4.0,  // max
        0.05, // step
        0.5,  // page
        0.0,  // page_size
    );
    let scale_row = adw::SpinRow::builder()
        .title(t("display-scale"))
        .adjustment(&scale_adjustment)
        .digits(2)
        .build();
    group.add(&scale_row);

    let scale_slider = gtk::Scale::builder()
        .orientation(gtk::Orientation::Horizontal)
        .adjustment(&scale_adjustment)
        .hexpand(true)
        .draw_value(false)
        .build();

    // Add marks at 0.5 intervals for visual snap targets
    let mut mark = 0.5;
    while mark <= 4.0 {
        scale_slider.add_mark(mark, gtk::PositionType::Bottom, None::<&str>);
        mark += 0.5;
    }

    // Snap slider drag to nearest 0.5
    {
        let adj = scale_adjustment.clone();
        scale_slider.connect_change_value(move |_scale, _scroll_type, value| {
            let snapped = (value * 2.0).round() / 2.0;
            let clamped = snapped.clamp(0.5, 4.0);
            adj.set_value(clamped);
            gtk::glib::Propagation::Stop
        });
    }

    let scale_slider_row = adw::ActionRow::new();
    scale_slider_row.add_suffix(&scale_slider);
    group.add(&scale_slider_row);

    // Buffer scale changes into pending state
    {
        let urn_str = urn.as_str().to_string();
        let pending_ref = pending.clone();
        let guard = updating.clone();
        let apply_btn = apply_button.clone();
        let reset_btn = reset_button.clone();
        scale_adjustment.connect_value_changed(move |adj| {
            if guard.get() {
                return;
            }
            let value = adj.value();
            let mut map = pending_ref.borrow_mut();
            let entry = map.entry(urn_str.clone()).or_default();
            entry.scale = Some(value);
            drop(map);
            apply_btn.set_sensitive(true);
            reset_btn.set_sensitive(true);
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

    // Wire rotation change -> buffer pending transform
    {
        let urn_str = urn.as_str().to_string();
        let pending_ref = pending.clone();
        let guard = updating.clone();
        let flip_ref = flip_row.clone();
        let apply_btn = apply_button.clone();
        let reset_btn = reset_button.clone();
        rotation_row.connect_selected_notify(move |row| {
            if guard.get() {
                return;
            }
            let rot = row.selected() as usize;
            let flip = flip_ref.is_active();
            let transform = DisplayTransform::compose(rot, flip);
            let mut map = pending_ref.borrow_mut();
            let entry = map.entry(urn_str.clone()).or_default();
            entry.transform = Some(transform);
            drop(map);
            apply_btn.set_sensitive(true);
            reset_btn.set_sensitive(true);
        });
    }

    // Wire flip change -> buffer pending transform
    {
        let urn_str = urn.as_str().to_string();
        let pending_ref = pending.clone();
        let guard = updating.clone();
        let rot_ref = rotation_row.clone();
        let apply_btn = apply_button.clone();
        let reset_btn = reset_button.clone();
        flip_row.connect_active_notify(move |row| {
            if guard.get() {
                return;
            }
            let rot = rot_ref.selected() as usize;
            let flip = row.is_active();
            let transform = DisplayTransform::compose(rot, flip);
            let mut map = pending_ref.borrow_mut();
            let entry = map.entry(urn_str.clone()).or_default();
            entry.transform = Some(transform);
            drop(map);
            apply_btn.set_sensitive(true);
            reset_btn.set_sensitive(true);
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
        let urn_str = urn.as_str().to_string();
        let pending_ref = pending.clone();
        let guard = updating.clone();
        let apply_btn = apply_button.clone();
        let reset_btn = reset_button.clone();
        vrr_row.connect_active_notify(move |row| {
            if guard.get() {
                return;
            }
            let mut map = pending_ref.borrow_mut();
            let entry = map.entry(urn_str.clone()).or_default();
            entry.vrr = Some(row.is_active());
            drop(map);
            apply_btn.set_sensitive(true);
            reset_btn.set_sensitive(true);
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
        scale_slider,
        scale_adjustment,
        rotation_row,
        flip_row,
        vrr_row,
        updating,
        resolutions: resolutions_rc,
        rate_mode_indices: rate_mode_indices_rc,
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

    #[test]
    fn test_pending_output_changes_dirty() {
        let empty = PendingOutputChanges::default();
        assert!(!empty.dirty());

        let with_scale = PendingOutputChanges {
            scale: Some(1.5),
            ..Default::default()
        };
        assert!(with_scale.dirty());

        let with_mode = PendingOutputChanges {
            mode_index: Some(0),
            ..Default::default()
        };
        assert!(with_mode.dirty());

        let with_transform = PendingOutputChanges {
            transform: Some(DisplayTransform::Normal),
            ..Default::default()
        };
        assert!(with_transform.dirty());

        let with_vrr = PendingOutputChanges {
            vrr: Some(true),
            ..Default::default()
        };
        assert!(with_vrr.dirty());

        let with_enabled = PendingOutputChanges {
            enabled: Some(false),
            ..Default::default()
        };
        assert!(with_enabled.dirty());
    }

    #[test]
    fn test_any_dirty() {
        let empty: HashMap<String, PendingOutputChanges> = HashMap::new();
        assert!(!any_dirty(&empty));

        let mut with_clean = HashMap::new();
        with_clean.insert("dp-1".to_string(), PendingOutputChanges::default());
        assert!(!any_dirty(&with_clean));

        let mut with_dirty = HashMap::new();
        with_dirty.insert(
            "dp-1".to_string(),
            PendingOutputChanges {
                scale: Some(2.0),
                ..Default::default()
            },
        );
        assert!(any_dirty(&with_dirty));
    }

    #[test]
    fn test_format_resolution() {
        assert_eq!(format_resolution(1920, 1080), "1920\u{00D7}1080");
        assert_eq!(format_resolution(2560, 1440), "2560\u{00D7}1440");
        assert_eq!(format_resolution(3840, 2160), "3840\u{00D7}2160");
    }

    #[test]
    fn test_distinct_resolutions_empty() {
        let result = distinct_resolutions(&[]);
        assert!(result.is_empty());
    }

    #[test]
    fn test_distinct_resolutions_single() {
        let modes = vec![DisplayMode {
            width: 1920,
            height: 1080,
            refresh_rate: 60.0,
            preferred: true,
        }];
        let result = distinct_resolutions(&modes);
        assert_eq!(result, vec![(1920, 1080)]);
    }

    #[test]
    fn test_display_title_model_only() {
        let output = DisplayOutput {
            name: "DP-1".to_string(),
            make: String::new(),
            model: "LS49AG95".to_string(),
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
        assert_eq!(display_title(&output), "LS49AG95");
    }
}
