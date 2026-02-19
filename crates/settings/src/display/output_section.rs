//! Display output settings section -- smart container.
//!
//! Subscribes to `EntityStore` for `display-output` entity type.
//! Renders one preferences group per display output with:
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
//! Reset discards pending changes and re-reconciles from entity store.

use std::cell::Cell;
use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::rc::Rc;

use adw::prelude::*;
use waft_client::{EntityActionCallback, EntityStore};

use crate::i18n::t;
use crate::i18n::t_args;
use crate::search_index::SearchIndex;
use waft_protocol::Urn;
use waft_protocol::entity::display::{
    DISPLAY_OUTPUT_ENTITY_TYPE, DisplayMode, DisplayOutput, DisplayTransform,
};

/// Pending (uncommitted) display output changes for one URN.
#[derive(Default)]
struct PendingOutputChanges {
    /// New mode index to apply (from resolution or refresh rate change).
    mode_index: Option<usize>,
    /// New scale value.
    scale: Option<f64>,
    /// New transform (combines rotation + flip).
    transform: Option<DisplayTransform>,
    /// New VRR state.
    vrr: Option<bool>,
    /// New enabled state.
    enabled: Option<bool>,
}

impl PendingOutputChanges {
    fn dirty(&self) -> bool {
        self.mode_index.is_some()
            || self.scale.is_some()
            || self.transform.is_some()
            || self.vrr.is_some()
            || self.enabled.is_some()
    }
}

/// Smart container for display output settings.
pub struct OutputSection {
    pub root: gtk::Box,
    pending: Rc<RefCell<HashMap<String, PendingOutputChanges>>>,
    entity_store: Rc<EntityStore>,
    outputs: Rc<RefCell<HashMap<String, OutputGroupWidgets>>>,
    apply_button: gtk::Button,
    reset_button: gtk::Button,
}

struct OutputGroupWidgets {
    group: adw::PreferencesGroup,
    enable_row: adw::SwitchRow,
    connection_row: adw::ActionRow,
    physical_size_row: adw::ActionRow,
    resolution_row: adw::ComboRow,
    refresh_rate_row: adw::ComboRow,
    scale_row: adw::SpinRow,
    #[allow(dead_code)] // Kept for ownership; updates via shared adjustment
    scale_slider: gtk::Scale,
    #[allow(dead_code)] // Kept for ownership; shared between SpinRow and Scale
    scale_adjustment: gtk::Adjustment,
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

/// Check if any output has pending changes.
fn any_dirty(pending: &HashMap<String, PendingOutputChanges>) -> bool {
    pending.values().any(|p| p.dirty())
}

impl OutputSection {
    pub fn new(
        entity_store: &Rc<EntityStore>,
        action_callback: &EntityActionCallback,
        search_index: &Rc<RefCell<SearchIndex>>,
    ) -> Self {
        let root = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .spacing(24)
            .visible(false)
            .build();

        let outputs: Rc<RefCell<HashMap<String, OutputGroupWidgets>>> =
            Rc::new(RefCell::new(HashMap::new()));
        let pending: Rc<RefCell<HashMap<String, PendingOutputChanges>>> =
            Rc::new(RefCell::new(HashMap::new()));

        // --- Apply / Reset buttons ---
        let button_box = gtk::Box::builder()
            .orientation(gtk::Orientation::Horizontal)
            .spacing(12)
            .halign(gtk::Align::End)
            .build();

        let reset_button = gtk::Button::builder()
            .label(t("display-reset"))
            .css_classes(["destructive-action"])
            .sensitive(false)
            .build();

        let apply_button = gtk::Button::builder()
            .label(t("display-apply"))
            .css_classes(["suggested-action"])
            .sensitive(false)
            .build();

        button_box.append(&reset_button);
        button_box.append(&apply_button);

        let button_group = adw::PreferencesGroup::new();
        button_group.add(&button_box);
        // button_group is appended after all output groups; we add it at the end
        // We use a separate container approach -- the button group is always last in root

        // We'll append button_group to root. Output groups are also appended to root.
        // To ensure button_group is always at the bottom, we append it now and insert
        // output groups before it. Actually, let's just append it last and manage ordering.
        // Since we only add output groups via root.append, and button_group is appended once,
        // new output groups will appear before button_group only if we insert them before.
        // GTK Box doesn't support insert_before easily. Instead, let's use a content_box
        // for output groups and put button_group after it.

        let content_box = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .spacing(24)
            .build();
        root.append(&content_box);
        root.append(&button_group);

        // Wire Apply button
        {
            let pending_ref = pending.clone();
            let outputs_ref = outputs.clone();
            let cb = action_callback.clone();
            let store = entity_store.clone();
            let apply_btn = apply_button.clone();
            let reset_btn = reset_button.clone();
            apply_button.connect_clicked(move |btn| {
                Self::apply_all(&pending_ref, &outputs_ref, &cb, &store);
                apply_btn.set_sensitive(false);
                reset_btn.set_sensitive(false);
                btn.grab_focus();
            });
        }

        // Wire Reset button
        {
            let pending_ref = pending.clone();
            let outputs_ref = outputs.clone();
            let store = entity_store.clone();
            let content_ref = content_box.clone();
            let apply_btn = apply_button.clone();
            let reset_btn = reset_button.clone();
            let root_ref = root.clone();
            let apply_for_reconcile = apply_button.clone();
            let reset_for_reconcile = reset_button.clone();
            reset_button.connect_clicked(move |btn| {
                pending_ref.borrow_mut().clear();
                apply_btn.set_sensitive(false);
                reset_btn.set_sensitive(false);
                let entities: Vec<(Urn, DisplayOutput)> =
                    store.get_entities_typed(DISPLAY_OUTPUT_ENTITY_TYPE);
                Self::reconcile(&outputs_ref, &content_ref, &entities, &pending_ref, &apply_for_reconcile, &reset_for_reconcile);
                root_ref.set_visible(!entities.is_empty());
                btn.grab_focus();
            });
        }

        // Subscribe to display-output entities
        {
            let store = entity_store.clone();
            let content_ref = content_box.clone();
            let outputs_ref = outputs.clone();
            let pending_ref = pending.clone();
            let apply_btn = apply_button.clone();
            let reset_btn = reset_button.clone();
            let root_ref = root.clone();
            let idx_ref = search_index.clone();

            entity_store.subscribe_type(DISPLAY_OUTPUT_ENTITY_TYPE, move || {
                let entities: Vec<(Urn, DisplayOutput)> =
                    store.get_entities_typed(DISPLAY_OUTPUT_ENTITY_TYPE);
                Self::reconcile(&outputs_ref, &content_ref, &entities, &pending_ref, &apply_btn, &reset_btn);
                root_ref.set_visible(!entities.is_empty());

                // Re-register dynamic output search entries
                Self::register_output_search_entries(&idx_ref, &outputs_ref);
                let is_dirty = any_dirty(&pending_ref.borrow());
                apply_btn.set_sensitive(is_dirty);
                reset_btn.set_sensitive(is_dirty);
            });
        }

        // Initial reconciliation for cached entities
        {
            let store = entity_store.clone();
            let content_ref = content_box;
            let outputs_ref = outputs.clone();
            let pending_ref = pending.clone();
            let root_ref = root.clone();
            let apply_btn = apply_button.clone();
            let reset_btn = reset_button.clone();

            gtk::glib::idle_add_local_once(move || {
                let entities: Vec<(Urn, DisplayOutput)> =
                    store.get_entities_typed(DISPLAY_OUTPUT_ENTITY_TYPE);
                if !entities.is_empty() {
                    log::debug!(
                        "[output-section] Initial reconciliation: {} outputs",
                        entities.len()
                    );
                    Self::reconcile(&outputs_ref, &content_ref, &entities, &pending_ref, &apply_btn, &reset_btn);
                    root_ref.set_visible(true);
                }
            });
        }

        Self {
            root,
            pending,
            entity_store: entity_store.clone(),
            outputs,
            apply_button,
            reset_button,
        }
    }

    /// Discard all pending changes and re-reconcile from entity store.
    pub fn reset(&self) {
        self.pending.borrow_mut().clear();
        self.apply_button.set_sensitive(false);
        self.reset_button.set_sensitive(false);

        let entities: Vec<(Urn, DisplayOutput)> =
            self.entity_store.get_entities_typed(DISPLAY_OUTPUT_ENTITY_TYPE);

        // The content_box is the first child of root.
        if let Some(content_box) = self.root.first_child()
            && let Ok(content) = content_box.downcast::<gtk::Box>()
        {
            Self::reconcile(
                &self.outputs,
                &content,
                &entities,
                &self.pending,
                &self.apply_button,
                &self.reset_button,
            );
        }
    }

    fn reconcile(
        outputs_map: &Rc<RefCell<HashMap<String, OutputGroupWidgets>>>,
        content_box: &gtk::Box,
        entities: &[(Urn, DisplayOutput)],
        pending: &Rc<RefCell<HashMap<String, PendingOutputChanges>>>,
        apply_button: &gtk::Button,
        reset_button: &gtk::Button,
    ) {
        let mut map = outputs_map.borrow_mut();
        let mut seen = HashSet::new();
        let pending_map = pending.borrow();

        // Count enabled outputs for at-least-one-active enforcement
        // Take pending enabled state into account
        let enabled_count = entities
            .iter()
            .filter(|(urn, o)| {
                let urn_str = urn.as_str().to_string();
                if let Some(p) = pending_map.get(&urn_str) {
                    p.enabled.unwrap_or(o.enabled)
                } else {
                    o.enabled
                }
            })
            .count();

        for (urn, output) in entities {
            let urn_str = urn.as_str().to_string();
            seen.insert(urn_str.clone());

            let output_pending = pending_map.get(&urn_str);

            if let Some(existing) = map.get(&urn_str) {
                Self::update_output_group(existing, output, enabled_count, output_pending);
            } else {
                let widgets = Self::create_output_group(urn, output, pending, apply_button, reset_button);
                // Set enable switch sensitivity based on at-least-one-active rule
                if output.enabled && enabled_count <= 1 {
                    widgets.enable_row.set_sensitive(false);
                }
                content_box.append(&widgets.group);
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
                content_box.remove(&widgets.group);
            }
        }
    }

    fn update_output_group(
        widgets: &OutputGroupWidgets,
        output: &DisplayOutput,
        enabled_count: usize,
        pending: Option<&PendingOutputChanges>,
    ) {
        widgets.updating.set(true);

        // Title & description (always update -- read-only)
        widgets.group.set_title(&display_title(output));
        widgets
            .group
            .set_description(Some(&t_args("display-output-name", &[("name", &output.name)])));

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

    fn create_output_group(
        urn: &Urn,
        output: &DisplayOutput,
        pending: &Rc<RefCell<HashMap<String, PendingOutputChanges>>>,
        apply_button: &gtk::Button,
        reset_button: &gtk::Button,
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
                            // Set a placeholder; the rate dropdown will be fully rebuilt
                            // when the entity update arrives after Apply
                            rate_list.append(&format!("{}\u{00D7}{}", w, h));
                            rate_row.set_model(Some(&rate_list));
                            rate_row.set_selected(0);

                            guard_inner.set(false);

                            // Buffer pending mode change
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

    /// Re-register dynamic search entries for all current display outputs.
    fn register_output_search_entries(
        search_index: &Rc<RefCell<SearchIndex>>,
        outputs: &Rc<RefCell<HashMap<String, OutputGroupWidgets>>>,
    ) {
        let mut idx = search_index.borrow_mut();
        let page_title = t("settings-display");
        // Remove old dynamic output entries — they share section_title with
        // the display name, so we remove all non-brightness display entries
        // by removing and re-adding per output.
        // We use a convention: output section titles are the display title (make+model).
        // First remove all display page entries that are not the page-level or brightness section.
        // Simpler: just remove all entries we're about to re-add by iterating outputs.
        let map = outputs.borrow();
        // Remove entries for each output by its title
        for widgets in map.values() {
            let title = widgets.group.title().to_string();
            idx.remove_entries("display", &title);
        }
        // Re-register
        for widgets in map.values() {
            let title = widgets.group.title().to_string();
            idx.add_section("display", &page_title, &title, "display-output", &widgets.group);
            idx.add_input("display", &page_title, &title, &t("display-resolution"), "display-resolution", &widgets.resolution_row);
            idx.add_input("display", &page_title, &title, &t("display-refresh-rate"), "display-refresh-rate", &widgets.refresh_rate_row);
            idx.add_input("display", &page_title, &title, &t("display-scale"), "display-scale", &widgets.scale_row);
            idx.add_input("display", &page_title, &title, &t("display-rotation"), "display-rotation", &widgets.rotation_row);
            idx.add_input("display", &page_title, &title, &t("display-flip"), "display-flip", &widgets.flip_row);
            idx.add_input("display", &page_title, &title, &t("display-vrr"), "display-vrr", &widgets.vrr_row);
        }
    }

    /// Apply all pending changes by firing entity actions, then clear pending state.
    fn apply_all(
        pending: &Rc<RefCell<HashMap<String, PendingOutputChanges>>>,
        outputs: &Rc<RefCell<HashMap<String, OutputGroupWidgets>>>,
        action_callback: &EntityActionCallback,
        entity_store: &Rc<EntityStore>,
    ) {
        let mut pending_map = pending.borrow_mut();
        let entities: Vec<(Urn, DisplayOutput)> =
            entity_store.get_entities_typed(DISPLAY_OUTPUT_ENTITY_TYPE);

        // Build a lookup from urn_str -> (Urn, DisplayOutput)
        let entity_lookup: HashMap<String, (&Urn, &DisplayOutput)> = entities
            .iter()
            .map(|(urn, output)| (urn.as_str().to_string(), (urn, output)))
            .collect();

        // Lock outputs to check VRR state for toggle logic
        let outputs_map = outputs.borrow();

        for (urn_str, changes) in pending_map.drain() {
            let urn = match entity_lookup.get(&urn_str) {
                Some((urn, _)) => (*urn).clone(),
                None => {
                    log::warn!("[output-section] Pending changes for unknown URN: {urn_str}");
                    continue;
                }
            };
            let current_output = entity_lookup.get(&urn_str).map(|(_, o)| *o);

            if let Some(enabled) = changes.enabled {
                action_callback(
                    urn.clone(),
                    "set-enabled".to_string(),
                    serde_json::json!({ "value": enabled }),
                );
            }

            if let Some(mode_index) = changes.mode_index {
                action_callback(
                    urn.clone(),
                    "set-mode".to_string(),
                    serde_json::json!({ "mode_index": mode_index }),
                );
            }

            if let Some(scale) = changes.scale {
                action_callback(
                    urn.clone(),
                    "set-scale".to_string(),
                    serde_json::json!({ "value": scale }),
                );
            }

            if let Some(transform) = changes.transform {
                action_callback(
                    urn.clone(),
                    "set-transform".to_string(),
                    serde_json::json!({ "value": transform }),
                );
            }

            if let Some(vrr) = changes.vrr {
                // VRR is a toggle action -- only fire if the desired state differs from current
                let current_vrr = current_output.map(|o| o.vrr_enabled).unwrap_or(false);
                if vrr != current_vrr {
                    action_callback(
                        urn.clone(),
                        "toggle-vrr".to_string(),
                        serde_json::Value::Null,
                    );
                }
            }

            // Reset the updating guard for widgets that had pending changes
            if let Some(widgets) = outputs_map.get(&urn_str) {
                widgets.updating.set(false);
            }
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
}
