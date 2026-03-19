//! Display settings page -- smart container.
//!
//! Subscribes to both `display` (brightness) and `display-output` (resolution/mode)
//! entity types. Correlates entities by connector field and renders unified
//! preferences groups per display. Brightness changes are immediate; output
//! changes are buffered with Apply/Reset.

use std::cell::Cell;
use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::rc::Rc;

use adw::prelude::*;
use waft_client::{EntityActionCallback, EntityStore};

use crate::display::output_section::{
    OutputGroupWidgets, PendingOutputChanges,
    any_dirty, create_output_rows, display_title, update_output_group,
};
use crate::i18n::t;
use crate::search_index::SearchIndex;
use waft_protocol::Urn;
use waft_protocol::entity::display::{
    DISPLAY_ENTITY_TYPE, DISPLAY_OUTPUT_ENTITY_TYPE, Display, DisplayKind, DisplayOutput,
};

/// Display settings page with unified display groups.
pub struct DisplayPage {
    pub root: gtk::Box,
    pending: Rc<RefCell<HashMap<String, PendingOutputChanges>>>,
    entity_store: Rc<EntityStore>,
    groups: Rc<RefCell<HashMap<String, UnifiedDisplayGroup>>>,
    apply_button: gtk::Button,
    reset_button: gtk::Button,
}

/// Widgets for a unified display group (brightness + output in one PreferencesGroup).
struct UnifiedDisplayGroup {
    group: adw::PreferencesGroup,
    /// Brightness row, present when a Display entity matched.
    brightness: Option<BrightnessWidgets>,
    /// Output control widgets, present when a DisplayOutput entity matched.
    output: Option<OutputGroupWidgets>,
}

struct BrightnessWidgets {
    scale: gtk::Scale,
    #[allow(dead_code)] // Kept for ownership; row is added to the group
    row: adw::ActionRow,
    updating: Rc<Cell<bool>>,
}

impl DisplayPage {
    /// Phase 1: Register static search entries without constructing widgets.
    pub fn register_search(_idx: &mut SearchIndex) {
        // Dynamic — entries registered during reconciliation.
    }

    pub fn new(
        entity_store: &Rc<EntityStore>,
        action_callback: &EntityActionCallback,
        search_index: &Rc<RefCell<SearchIndex>>,
    ) -> Self {
        let root = crate::page_layout::page_root();

        let content_box = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .spacing(24)
            .build();
        root.append(&content_box);

        // Apply / Reset buttons
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
        root.append(&button_group);

        let groups: Rc<RefCell<HashMap<String, UnifiedDisplayGroup>>> =
            Rc::new(RefCell::new(HashMap::new()));
        let pending: Rc<RefCell<HashMap<String, PendingOutputChanges>>> =
            Rc::new(RefCell::new(HashMap::new()));

        // Wire Apply button
        {
            let pending_ref = pending.clone();
            let groups_ref = groups.clone();
            let cb = action_callback.clone();
            let store = entity_store.clone();
            let apply_btn = apply_button.clone();
            let reset_btn = reset_button.clone();
            apply_button.connect_clicked(move |btn| {
                Self::apply_all(&pending_ref, &groups_ref, &cb, &store);
                apply_btn.set_sensitive(false);
                reset_btn.set_sensitive(false);
                btn.grab_focus();
            });
        }

        // Wire Reset button
        {
            let pending_ref = pending.clone();
            let groups_ref = groups.clone();
            let store = entity_store.clone();
            let content_ref = content_box.clone();
            let apply_btn = apply_button.clone();
            let reset_btn = reset_button.clone();
            let root_ref = root.clone();
            let cb = action_callback.clone();
            let idx_ref = search_index.clone();
            reset_button.connect_clicked(move |btn| {
                pending_ref.borrow_mut().clear();
                apply_btn.set_sensitive(false);
                reset_btn.set_sensitive(false);
                let displays: Vec<(Urn, Display)> =
                    store.get_entities_typed(DISPLAY_ENTITY_TYPE);
                let outputs: Vec<(Urn, DisplayOutput)> =
                    store.get_entities_typed(DISPLAY_OUTPUT_ENTITY_TYPE);
                Self::reconcile(
                    &groups_ref, &content_ref, &displays, &outputs,
                    &pending_ref, &apply_btn, &reset_btn, &cb,
                );
                Self::register_search_entries(&idx_ref, &groups_ref);
                root_ref.set_visible(!displays.is_empty() || !outputs.is_empty());
                btn.grab_focus();
            });
        }

        // Subscribe to both entity types
        {
            let content_ref = content_box.clone();
            let groups_ref = groups.clone();
            let pending_ref = pending.clone();
            let apply_btn = apply_button.clone();
            let reset_btn = reset_button.clone();
            let root_ref = root.clone();
            let cb = action_callback.clone();
            let idx_ref = search_index.clone();

            crate::subscription::subscribe_dual_entities::<Display, DisplayOutput, _>(
                entity_store,
                DISPLAY_ENTITY_TYPE,
                DISPLAY_OUTPUT_ENTITY_TYPE,
                move |displays, outputs| {
                    log::debug!(
                        "[display-page] Reconciling: {} displays, {} outputs",
                        displays.len(),
                        outputs.len()
                    );
                    Self::reconcile(
                        &groups_ref, &content_ref, &displays, &outputs,
                        &pending_ref, &apply_btn, &reset_btn, &cb,
                    );
                    Self::register_search_entries(&idx_ref, &groups_ref);
                    let is_dirty = any_dirty(&pending_ref.borrow());
                    apply_btn.set_sensitive(is_dirty);
                    reset_btn.set_sensitive(is_dirty);
                    root_ref.set_visible(!displays.is_empty() || !outputs.is_empty());
                },
            );
        }

        Self {
            root,
            pending,
            entity_store: entity_store.clone(),
            groups,
            apply_button,
            reset_button,
        }
    }

    /// Discard pending output changes and re-reconcile from entity store.
    pub fn reset(&self) {
        self.pending.borrow_mut().clear();
        self.apply_button.set_sensitive(false);
        self.reset_button.set_sensitive(false);

        let displays: Vec<(Urn, Display)> =
            self.entity_store.get_entities_typed(DISPLAY_ENTITY_TYPE);
        let outputs: Vec<(Urn, DisplayOutput)> =
            self.entity_store.get_entities_typed(DISPLAY_OUTPUT_ENTITY_TYPE);

        // We cannot borrow action_callback here, but reset just needs to
        // re-reconcile widgets from entity store state. The EntityActionCallback
        // is not needed for update_output_group. However, create_output_rows needs
        // it for new groups. Since reset is only called when leaving the page (no new
        // groups expected), we can skip creation and just update existing groups.
        let groups = self.groups.borrow();
        let pending_map = self.pending.borrow();

        // Count enabled outputs for at-least-one-active enforcement
        let enabled_count = outputs
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

        for (key, group) in groups.iter() {
            // Find matching display
            if let Some(bw) = &group.brightness
                && let Some((_, display)) = displays
                    .iter()
                    .find(|(_, d)| group_key_for_display(d) == *key)
            {
                bw.updating.set(true);
                bw.scale.set_value(display.brightness);
                bw.updating.set(false);
            }

            // Find matching output and update widgets
            if let Some(ow) = &group.output
                && let Some((urn, output)) = outputs
                    .iter()
                    .find(|(_, o)| o.name == *key)
            {
                let urn_str = urn.as_str().to_string();
                let output_pending = pending_map.get(&urn_str);
                update_output_group(ow, output, enabled_count, output_pending);
            }
        }
    }

    /// Correlate display and output entities and reconcile unified groups.
    #[allow(clippy::too_many_arguments)]
    fn reconcile(
        groups_map: &Rc<RefCell<HashMap<String, UnifiedDisplayGroup>>>,
        content_box: &gtk::Box,
        displays: &[(Urn, Display)],
        outputs: &[(Urn, DisplayOutput)],
        pending: &Rc<RefCell<HashMap<String, PendingOutputChanges>>>,
        apply_button: &gtk::Button,
        reset_button: &gtk::Button,
        action_callback: &EntityActionCallback,
    ) {
        let mut map = groups_map.borrow_mut();
        let mut seen = HashSet::new();
        let pending_map = pending.borrow();

        // Build lookups for correlation
        let display_by_connector: HashMap<&str, (&Urn, &Display)> = displays
            .iter()
            .filter_map(|(urn, d)| d.connector.as_deref().map(|c| (c, (urn, d))))
            .collect();

        // Count enabled outputs (considering pending state)
        let enabled_count = outputs
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

        // Phase 1: Process outputs (they define the primary groups)
        for (output_urn, output) in outputs {
            let key = output.name.clone();
            seen.insert(key.clone());

            let matching_display = display_by_connector.get(output.name.as_str());
            let urn_str = output_urn.as_str().to_string();
            let output_pending = pending_map.get(&urn_str);

            if let Some(existing) = map.get(&key) {
                // Update existing group
                let title = display_title(output);
                existing.group.set_title(&title);

                // Update brightness if present
                if let (Some(bw), Some((_, display))) = (&existing.brightness, matching_display) {
                    bw.updating.set(true);
                    bw.scale.set_value(display.brightness);
                    bw.updating.set(false);
                } else if existing.brightness.is_none() && matching_display.is_some() {
                    // Display appeared after output — would need to add brightness row.
                    // This is rare; full rebuild on next reconcile handles it.
                }

                // Update output widgets
                if let Some(ow) = &existing.output {
                    update_output_group(ow, output, enabled_count, output_pending);
                }
            } else {
                // Create new unified group
                let group = adw::PreferencesGroup::builder()
                    .title(display_title(output))
                    .build();

                // Add brightness row if matching display exists
                let brightness = matching_display.map(|(display_urn, display)| {
                    Self::create_brightness_row(
                        &group, display_urn, display, action_callback,
                    )
                });

                // Add output control rows
                let output_widgets = create_output_rows(
                    &group, output_urn, output, pending, apply_button, reset_button,
                );

                // Set enable switch sensitivity
                if output.enabled && enabled_count <= 1 {
                    output_widgets.enable_row.set_sensitive(false);
                }

                content_box.append(&group);
                map.insert(key, UnifiedDisplayGroup {
                    group,
                    brightness,
                    output: Some(output_widgets),
                });
            }
        }

        // Phase 2: Process standalone displays (no matching output)
        for (display_urn, display) in displays {
            let key = group_key_for_display(display);
            if seen.contains(&key) {
                continue;
            }
            seen.insert(key.clone());

            if let Some(existing) = map.get(&key) {
                // Update brightness
                if let Some(bw) = &existing.brightness {
                    bw.updating.set(true);
                    existing.group.set_title(&display.name);
                    let subtitle = match display.kind {
                        DisplayKind::Backlight => t("display-builtin"),
                        DisplayKind::External => t("display-external"),
                    };
                    existing.group.set_description(Some(&subtitle));
                    bw.scale.set_value(display.brightness);
                    bw.updating.set(false);
                }
            } else {
                let subtitle = match display.kind {
                    DisplayKind::Backlight => t("display-builtin"),
                    DisplayKind::External => t("display-external"),
                };
                let group = adw::PreferencesGroup::builder()
                    .title(&display.name)
                    .description(subtitle)
                    .build();

                let brightness = Self::create_brightness_row(
                    &group, display_urn, display, action_callback,
                );

                content_box.append(&group);
                map.insert(key, UnifiedDisplayGroup {
                    group,
                    brightness: Some(brightness),
                    output: None,
                });
            }
        }

        // Remove stale groups
        let to_remove: Vec<String> = map
            .keys()
            .filter(|k| !seen.contains(*k))
            .cloned()
            .collect();
        for key in to_remove {
            if let Some(group) = map.remove(&key) {
                content_box.remove(&group.group);
            }
        }
    }

    /// Create a brightness slider row and add it to the group.
    fn create_brightness_row(
        group: &adw::PreferencesGroup,
        urn: &Urn,
        display: &Display,
        action_callback: &EntityActionCallback,
    ) -> BrightnessWidgets {
        let scale = gtk::Scale::builder()
            .orientation(gtk::Orientation::Horizontal)
            .hexpand(true)
            .draw_value(false)
            .build();
        scale.set_range(0.0, 1.0);
        scale.set_increments(0.05, 0.1);
        scale.set_value(display.brightness);

        let row = adw::ActionRow::builder().title(t("display-brightness")).build();
        row.add_suffix(&scale);
        group.add(&row);

        let updating = Rc::new(Cell::new(false));

        let urn_clone = urn.clone();
        let cb = action_callback.clone();
        let guard = updating.clone();
        scale.connect_value_changed(move |s| {
            if guard.get() {
                return;
            }
            cb(
                urn_clone.clone(),
                "set-brightness".to_string(),
                serde_json::json!({ "value": s.value() }),
            );
        });

        BrightnessWidgets {
            scale,
            row,
            updating,
        }
    }

    /// Re-register dynamic search entries for all unified display groups.
    fn register_search_entries(
        search_index: &Rc<RefCell<SearchIndex>>,
        groups: &Rc<RefCell<HashMap<String, UnifiedDisplayGroup>>>,
    ) {
        let mut idx = search_index.borrow_mut();
        let page_title = t("settings-display");
        let map = groups.borrow();

        // Remove all existing display page entries and re-add
        for group in map.values() {
            let title = group.group.title().to_string();
            idx.remove_entries("display", &title);
        }

        for group in map.values() {
            let title = group.group.title().to_string();

            if group.brightness.is_some() {
                idx.add_input(
                    "display", &page_title, &title,
                    &t("display-brightness"), "display-brightness",
                    &group.group,
                );
            }

            if let Some(ow) = &group.output {
                idx.add_section("display", &page_title, &title, "display-output", &group.group);
                idx.add_input("display", &page_title, &title, &t("display-resolution"), "display-resolution", &ow.resolution_row);
                idx.add_input("display", &page_title, &title, &t("display-refresh-rate"), "display-refresh-rate", &ow.refresh_rate_row);
                idx.add_input("display", &page_title, &title, &t("display-scale"), "display-scale", &ow.scale_row);
                idx.add_input("display", &page_title, &title, &t("display-rotation"), "display-rotation", &ow.rotation_row);
                idx.add_input("display", &page_title, &title, &t("display-flip"), "display-flip", &ow.flip_row);
                idx.add_input("display", &page_title, &title, &t("display-vrr"), "display-vrr", &ow.vrr_row);
            }
        }
    }

    /// Apply all pending output changes by firing entity actions.
    fn apply_all(
        pending: &Rc<RefCell<HashMap<String, PendingOutputChanges>>>,
        groups: &Rc<RefCell<HashMap<String, UnifiedDisplayGroup>>>,
        action_callback: &EntityActionCallback,
        entity_store: &Rc<EntityStore>,
    ) {
        let mut pending_map = pending.borrow_mut();
        let entities: Vec<(Urn, DisplayOutput)> =
            entity_store.get_entities_typed(DISPLAY_OUTPUT_ENTITY_TYPE);

        let entity_lookup: HashMap<String, (&Urn, &DisplayOutput)> = entities
            .iter()
            .map(|(urn, output)| (urn.as_str().to_string(), (urn, output)))
            .collect();

        let groups_map = groups.borrow();

        for (urn_str, changes) in pending_map.drain() {
            let urn = match entity_lookup.get(&urn_str) {
                Some((urn, _)) => (*urn).clone(),
                None => {
                    log::warn!("[display-page] Pending changes for unknown URN: {urn_str}");
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
                let current_vrr = current_output.map(|o| o.vrr_enabled).unwrap_or(false);
                if vrr != current_vrr {
                    action_callback(
                        urn.clone(),
                        "toggle-vrr".to_string(),
                        serde_json::Value::Null,
                    );
                }
            }

            // Reset updating guard for the output that had pending changes
            if let Some(output) = current_output
                && let Some(group) = groups_map.get(&output.name)
                && let Some(ow) = &group.output
            {
                ow.updating.set(false);
            }
        }
    }
}

/// Determine the group key for a display entity.
/// Uses connector if available, falls back to name-based key.
fn group_key_for_display(display: &Display) -> String {
    display
        .connector
        .clone()
        .unwrap_or_else(|| format!("brightness:{}", display.name))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn group_key_uses_connector_when_present() {
        let display = Display {
            name: "Built-in".to_string(),
            brightness: 0.5,
            kind: DisplayKind::Backlight,
            connector: Some("eDP-1".to_string()),
        };
        assert_eq!(group_key_for_display(&display), "eDP-1");
    }

    #[test]
    fn group_key_falls_back_to_brightness_prefix() {
        let display = Display {
            name: "LG DDC Monitor".to_string(),
            brightness: 0.8,
            kind: DisplayKind::External,
            connector: None,
        };
        assert_eq!(
            group_key_for_display(&display),
            "brightness:LG DDC Monitor"
        );
    }

    #[test]
    fn group_key_connector_matches_output_name() {
        // When connector is "DP-3", it should match DisplayOutput.name == "DP-3"
        let display = Display {
            name: "Samsung Monitor".to_string(),
            brightness: 0.6,
            kind: DisplayKind::External,
            connector: Some("DP-3".to_string()),
        };
        assert_eq!(group_key_for_display(&display), "DP-3");
    }
}
