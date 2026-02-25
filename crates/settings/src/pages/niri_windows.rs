//! Windows settings page -- smart container.
//!
//! Loads niri layout config and wires 7 dumb widget sections plus a
//! prefer-no-csd toggle and derive-colours toggle. Saves changes back to KDL.
//! Subscribes to `gtk-appearance` entity for derive-colours availability.

use std::cell::RefCell;
use std::rc::Rc;

use adw::prelude::*;
use waft_client::EntityStore;
use waft_protocol::entity::appearance::{GtkAppearance, GTK_APPEARANCE_ENTITY_TYPE};
use waft_protocol::Urn;

use crate::i18n::t;
use crate::kdl_config;
use crate::kdl_niri_windows::{self, NiriLayoutConfig};
use crate::niri_windows::border_section::{BorderSection, BorderSectionOutput};
use crate::niri_windows::derive_colors_section::{DeriveColorsSection, DeriveColorsSectionOutput};
use crate::niri_windows::focus_ring_section::{FocusRingSection, FocusRingSectionOutput};
use crate::niri_windows::gaps_section::{GapsSection, GapsSectionOutput};
use crate::niri_windows::shadow_section::{ShadowSection, ShadowSectionOutput};
use crate::niri_windows::struts_section::{StrutsSection, StrutsSectionOutput};
use crate::niri_windows::tab_indicator_section::{TabIndicatorSection, TabIndicatorSectionOutput};
use crate::prefs::SettingsPrefs;
use crate::search_index::SearchIndex;

/// Smart container for the Windows settings page.
pub struct NiriWindowsPage {
    pub root: gtk::Box,
}

impl NiriWindowsPage {
    pub fn new(entity_store: &Rc<EntityStore>, search_index: &Rc<RefCell<SearchIndex>>) -> Self {
        let root = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .spacing(24)
            .margin_top(24)
            .margin_bottom(24)
            .margin_start(12)
            .margin_end(12)
            .build();

        let config_path = kdl_config::niri_config_path();

        // Load layout config; on failure, show error state
        let layout_result = kdl_niri_windows::load_layout_config(&config_path);
        let prefer_no_csd_result = kdl_niri_windows::load_prefer_no_csd(&config_path);

        let (initial_layout, initial_no_csd) = match (&layout_result, &prefer_no_csd_result) {
            (Ok(layout), Ok(no_csd)) => (layout.clone(), *no_csd),
            _ => {
                // Show parse error state
                let status = adw::StatusPage::builder()
                    .icon_name("dialog-error-symbolic")
                    .title(t("windows-parse-error"))
                    .build();
                root.append(&status);
                return Self { root };
            }
        };

        let prefs = SettingsPrefs::load();
        let config_state: Rc<RefCell<NiriLayoutConfig>> =
            Rc::new(RefCell::new(initial_layout.clone()));

        // Helper to save current config state
        let save_config = {
            let path = config_path.clone();
            let state = config_state.clone();
            Rc::new(move || {
                let cfg = state.borrow();
                if let Err(e) = kdl_niri_windows::save_layout_config(&path, &cfg) {
                    log::warn!("[niri-windows] Failed to save layout config: {e}");
                }
            })
        };

        // -- Prefer No CSD toggle (inline, not a separate section) --
        let no_csd_group = adw::PreferencesGroup::builder().build();
        let no_csd_row = adw::SwitchRow::builder()
            .title(t("windows-prefer-no-csd"))
            .subtitle(t("windows-prefer-no-csd-sub"))
            .active(initial_no_csd)
            .build();
        no_csd_group.add(&no_csd_row);
        root.append(&no_csd_group);

        {
            let path = config_path.clone();
            no_csd_row.connect_active_notify(move |row| {
                if let Err(e) = kdl_niri_windows::save_prefer_no_csd(&path, row.is_active()) {
                    log::warn!("[niri-windows] Failed to save prefer-no-csd: {e}");
                }
            });
        }

        // -- Derive Colours section --
        let derive = Rc::new(DeriveColorsSection::new());
        derive.set_active(prefs.derive_window_colors_from_gtk);
        derive.set_available(false); // Will be set true by entity subscription
        root.append(&derive.root);

        // -- Focus Ring section --
        let focus_ring = Rc::new(FocusRingSection::new());
        focus_ring.apply_props(
            initial_layout.focus_ring.enabled,
            initial_layout.focus_ring.width,
            &initial_layout.focus_ring.active_color,
            &initial_layout.focus_ring.inactive_color,
            initial_layout.focus_ring.urgent_color.as_deref(),
        );
        root.append(&focus_ring.root);

        // -- Border section --
        let border = Rc::new(BorderSection::new());
        border.apply_props(
            initial_layout.border.enabled,
            initial_layout.border.width,
            &initial_layout.border.active_color,
            &initial_layout.border.inactive_color,
            initial_layout.border.urgent_color.as_deref(),
        );
        root.append(&border.root);

        // -- Shadow section --
        let shadow = Rc::new(ShadowSection::new());
        shadow.apply_props(
            initial_layout.shadow.enabled,
            initial_layout.shadow.softness,
            initial_layout.shadow.spread,
            initial_layout.shadow.offset_x,
            initial_layout.shadow.offset_y,
            &initial_layout.shadow.color,
            initial_layout.shadow.inactive_color.as_deref(),
        );
        root.append(&shadow.root);

        // -- Tab Indicator section --
        let tab_indicator = Rc::new(TabIndicatorSection::new());
        tab_indicator.apply_props(
            initial_layout.tab_indicator.enabled,
            &initial_layout.tab_indicator.position,
            initial_layout.tab_indicator.gap,
            initial_layout.tab_indicator.width,
            initial_layout.tab_indicator.corner_radius,
            &initial_layout.tab_indicator.active_color,
            &initial_layout.tab_indicator.inactive_color,
            initial_layout.tab_indicator.urgent_color.as_deref(),
        );
        root.append(&tab_indicator.root);

        // -- Gaps section --
        let gaps = Rc::new(GapsSection::new());
        gaps.apply_props(initial_layout.gaps);
        root.append(&gaps.root);

        // -- Struts section --
        let struts = Rc::new(StrutsSection::new());
        struts.apply_props(
            initial_layout.struts.left,
            initial_layout.struts.right,
            initial_layout.struts.top,
            initial_layout.struts.bottom,
        );
        root.append(&struts.root);

        // Set initial colour sensitivity based on derive preference
        if prefs.derive_window_colors_from_gtk {
            focus_ring.set_colors_sensitive(false);
            border.set_colors_sensitive(false);
            tab_indicator.set_colors_sensitive(false);
        }

        // -- Wire derive-colours toggle --
        {
            let save = save_config.clone();
            let state = config_state.clone();
            let store = entity_store.clone();
            let fr = focus_ring.clone();
            let bd = border.clone();
            let ti = tab_indicator.clone();
            derive.connect_output(move |output| {
                let DeriveColorsSectionOutput::Toggled(enabled) = output;

                // Save preference
                let mut p = SettingsPrefs::load();
                p.derive_window_colors_from_gtk = enabled;
                if let Err(e) = p.save() {
                    log::warn!("[niri-windows] Failed to save prefs: {e}");
                }

                fr.set_colors_sensitive(!enabled);
                bd.set_colors_sensitive(!enabled);
                ti.set_colors_sensitive(!enabled);

                if enabled {
                    // Look up current accent colour
                    let entities: Vec<(Urn, GtkAppearance)> =
                        store.get_entities_typed(GTK_APPEARANCE_ENTITY_TYPE);
                    if let Some((_, appearance)) = entities.first() {
                        apply_accent_palette(
                            &mut state.borrow_mut(),
                            &appearance.accent_color,
                            &fr,
                            &bd,
                            &ti,
                        );
                        save();
                    }
                }
            });
        }

        // -- Wire focus ring output --
        {
            let state = config_state.clone();
            let save = save_config.clone();
            focus_ring.connect_output(move |output| {
                let mut cfg = state.borrow_mut();
                match output {
                    FocusRingSectionOutput::Toggled(v) => cfg.focus_ring.enabled = v,
                    FocusRingSectionOutput::WidthChanged(v) => cfg.focus_ring.width = v,
                    FocusRingSectionOutput::ActiveColorChanged(v) => {
                        cfg.focus_ring.active_color = v
                    }
                    FocusRingSectionOutput::InactiveColorChanged(v) => {
                        cfg.focus_ring.inactive_color = v
                    }
                    FocusRingSectionOutput::UrgentColorChanged(v) => {
                        cfg.focus_ring.urgent_color = Some(v)
                    }
                }
                drop(cfg);
                save();
            });
        }

        // -- Wire border output --
        {
            let state = config_state.clone();
            let save = save_config.clone();
            border.connect_output(move |output| {
                let mut cfg = state.borrow_mut();
                match output {
                    BorderSectionOutput::Toggled(v) => cfg.border.enabled = v,
                    BorderSectionOutput::WidthChanged(v) => cfg.border.width = v,
                    BorderSectionOutput::ActiveColorChanged(v) => cfg.border.active_color = v,
                    BorderSectionOutput::InactiveColorChanged(v) => cfg.border.inactive_color = v,
                    BorderSectionOutput::UrgentColorChanged(v) => cfg.border.urgent_color = Some(v),
                }
                drop(cfg);
                save();
            });
        }

        // -- Wire shadow output --
        {
            let state = config_state.clone();
            let save = save_config.clone();
            shadow.connect_output(move |output| {
                let mut cfg = state.borrow_mut();
                match output {
                    ShadowSectionOutput::Toggled(v) => cfg.shadow.enabled = v,
                    ShadowSectionOutput::SoftnessChanged(v) => cfg.shadow.softness = v,
                    ShadowSectionOutput::SpreadChanged(v) => cfg.shadow.spread = v,
                    ShadowSectionOutput::OffsetXChanged(v) => cfg.shadow.offset_x = v,
                    ShadowSectionOutput::OffsetYChanged(v) => cfg.shadow.offset_y = v,
                    ShadowSectionOutput::ColorChanged(v) => cfg.shadow.color = v,
                    ShadowSectionOutput::InactiveColorChanged(v) => {
                        cfg.shadow.inactive_color = Some(v)
                    }
                }
                drop(cfg);
                save();
            });
        }

        // -- Wire tab indicator output --
        {
            let state = config_state.clone();
            let save = save_config.clone();
            tab_indicator.connect_output(move |output| {
                let mut cfg = state.borrow_mut();
                match output {
                    TabIndicatorSectionOutput::Toggled(v) => cfg.tab_indicator.enabled = v,
                    TabIndicatorSectionOutput::PositionChanged(v) => cfg.tab_indicator.position = v,
                    TabIndicatorSectionOutput::GapChanged(v) => cfg.tab_indicator.gap = v,
                    TabIndicatorSectionOutput::WidthChanged(v) => cfg.tab_indicator.width = v,
                    TabIndicatorSectionOutput::CornerRadiusChanged(v) => {
                        cfg.tab_indicator.corner_radius = v
                    }
                    TabIndicatorSectionOutput::ActiveColorChanged(v) => {
                        cfg.tab_indicator.active_color = v
                    }
                    TabIndicatorSectionOutput::InactiveColorChanged(v) => {
                        cfg.tab_indicator.inactive_color = v
                    }
                    TabIndicatorSectionOutput::UrgentColorChanged(v) => {
                        cfg.tab_indicator.urgent_color = Some(v)
                    }
                }
                drop(cfg);
                save();
            });
        }

        // -- Wire gaps output --
        {
            let state = config_state.clone();
            let save = save_config.clone();
            gaps.connect_output(move |output| {
                let GapsSectionOutput::GapsChanged(v) = output;
                state.borrow_mut().gaps = v;
                save();
            });
        }

        // -- Wire struts output --
        {
            let state = config_state.clone();
            let save = save_config.clone();
            struts.connect_output(move |output| {
                let mut cfg = state.borrow_mut();
                match output {
                    StrutsSectionOutput::LeftChanged(v) => cfg.struts.left = v,
                    StrutsSectionOutput::RightChanged(v) => cfg.struts.right = v,
                    StrutsSectionOutput::TopChanged(v) => cfg.struts.top = v,
                    StrutsSectionOutput::BottomChanged(v) => cfg.struts.bottom = v,
                }
                drop(cfg);
                save();
            });
        }

        // -- Subscribe to gtk-appearance for derive-colours availability --
        {
            let store = entity_store.clone();
            let derive_ref = derive.clone();
            let fr = focus_ring.clone();
            let bd = border.clone();
            let ti = tab_indicator.clone();
            let state = config_state.clone();
            let save = save_config.clone();

            entity_store.subscribe_type(GTK_APPEARANCE_ENTITY_TYPE, move || {
                let entities: Vec<(Urn, GtkAppearance)> =
                    store.get_entities_typed(GTK_APPEARANCE_ENTITY_TYPE);
                let available = !entities.is_empty();
                derive_ref.set_available(available);

                if available {
                    let prefs = SettingsPrefs::load();
                    if prefs.derive_window_colors_from_gtk {
                        if let Some((_, appearance)) = entities.first() {
                            apply_accent_palette(
                                &mut state.borrow_mut(),
                                &appearance.accent_color,
                                &fr,
                                &bd,
                                &ti,
                            );
                            save();
                        }
                    }
                }
            });
        }

        // -- Initial reconciliation for gtk-appearance --
        {
            let store = entity_store.clone();
            let derive_ref = derive.clone();
            let fr = focus_ring.clone();
            let bd = border.clone();
            let ti = tab_indicator.clone();
            let state = config_state;
            let save = save_config;

            gtk::glib::idle_add_local_once(move || {
                let entities: Vec<(Urn, GtkAppearance)> =
                    store.get_entities_typed(GTK_APPEARANCE_ENTITY_TYPE);
                if !entities.is_empty() {
                    log::debug!("[niri-windows] Initial reconciliation: gtk-appearance available");
                    derive_ref.set_available(true);

                    let prefs = SettingsPrefs::load();
                    if prefs.derive_window_colors_from_gtk {
                        if let Some((_, appearance)) = entities.first() {
                            apply_accent_palette(
                                &mut state.borrow_mut(),
                                &appearance.accent_color,
                                &fr,
                                &bd,
                                &ti,
                            );
                            save();
                        }
                    }
                }
            });
        }

        // Register search entries
        {
            let mut idx = search_index.borrow_mut();
            let page_title = t("settings-windows");
            idx.add_section(
                "windows",
                &page_title,
                &t("windows-prefer-no-csd"),
                "windows-prefer-no-csd",
                &no_csd_group,
            );
            idx.add_section(
                "windows",
                &page_title,
                &t("windows-derive-colors"),
                "windows-derive-colors",
                &derive.root,
            );
            idx.add_section(
                "windows",
                &page_title,
                &t("windows-focus-ring"),
                "windows-focus-ring",
                &focus_ring.root,
            );
            idx.add_section(
                "windows",
                &page_title,
                &t("windows-border"),
                "windows-border",
                &border.root,
            );
            idx.add_section(
                "windows",
                &page_title,
                &t("windows-shadow"),
                "windows-shadow",
                &shadow.root,
            );
            idx.add_section(
                "windows",
                &page_title,
                &t("windows-tab-indicator"),
                "windows-tab-indicator",
                &tab_indicator.root,
            );
            idx.add_section(
                "windows",
                &page_title,
                &t("windows-gaps"),
                "windows-gaps",
                &gaps.root,
            );
            idx.add_section(
                "windows",
                &page_title,
                &t("windows-struts"),
                "windows-struts",
                &struts.root,
            );
        }

        // Prevent sections from being dropped
        std::mem::forget(derive);
        std::mem::forget(focus_ring);
        std::mem::forget(border);
        std::mem::forget(shadow);
        std::mem::forget(tab_indicator);
        std::mem::forget(gaps);
        std::mem::forget(struts);

        Self { root }
    }
}

/// Map an accent colour name to concrete hex colours for focus-ring/border/tab-indicator.
/// Returns (active, inactive, urgent).
fn accent_palette(accent: &str) -> (&str, &str, &str) {
    match accent {
        "blue" => ("#3584e4", "#505050", "#e01b24"),
        "teal" => ("#2190a4", "#505050", "#e01b24"),
        "green" => ("#3a944a", "#505050", "#e01b24"),
        "yellow" => ("#c88800", "#505050", "#e01b24"),
        "orange" => ("#ed5b00", "#505050", "#e01b24"),
        "red" => ("#e62d42", "#505050", "#c88800"),
        "pink" => ("#d56199", "#505050", "#e01b24"),
        "purple" => ("#9141ac", "#505050", "#e01b24"),
        "slate" => ("#6f8396", "#505050", "#e01b24"),
        _ => ("#7fc8ff", "#505050", "#e01b24"),
    }
}

/// Apply derived accent colour palette to focus-ring, border, and tab-indicator
/// config and widget sections.
fn apply_accent_palette(
    cfg: &mut NiriLayoutConfig,
    accent: &str,
    focus_ring: &FocusRingSection,
    border: &BorderSection,
    tab_indicator: &TabIndicatorSection,
) {
    let (active, inactive, urgent) = accent_palette(accent);

    cfg.focus_ring.active_color = active.to_string();
    cfg.focus_ring.inactive_color = inactive.to_string();
    cfg.focus_ring.urgent_color = Some(urgent.to_string());

    cfg.border.active_color = active.to_string();
    cfg.border.inactive_color = inactive.to_string();
    cfg.border.urgent_color = Some(urgent.to_string());

    cfg.tab_indicator.active_color = active.to_string();
    cfg.tab_indicator.inactive_color = inactive.to_string();
    cfg.tab_indicator.urgent_color = Some(urgent.to_string());

    focus_ring.apply_props(
        cfg.focus_ring.enabled,
        cfg.focus_ring.width,
        active,
        inactive,
        Some(urgent),
    );
    border.apply_props(
        cfg.border.enabled,
        cfg.border.width,
        active,
        inactive,
        Some(urgent),
    );
    tab_indicator.apply_props(
        cfg.tab_indicator.enabled,
        &cfg.tab_indicator.position,
        cfg.tab_indicator.gap,
        cfg.tab_indicator.width,
        cfg.tab_indicator.corner_radius,
        active,
        inactive,
        Some(urgent),
    );
}
