//! Keyboard settings page -- smart container.
//!
//! Subscribes to `EntityStore` for `keyboard-layout-config` entity type.
//! On entity changes, reconciles keyboard layout list with drag-and-drop support.

use std::cell::RefCell;
use std::rc::Rc;

use adw::prelude::*;
use waft_client::{EntityActionCallback, EntityStore};
use waft_protocol::Urn;
use waft_protocol::entity::keyboard::{
    CONFIG_ENTITY_TYPE, ENTITY_TYPE as KEYBOARD_ENTITY_TYPE, KeyboardLayout, KeyboardLayoutConfig,
};
use waft_ui_gtk::widgets::ordered_list::{OrderedList, OrderedListOutput, OrderedListProps};
use waft_ui_gtk::widgets::ordered_list_row::{OrderedListRow, OrderedListRowProps};

use crate::i18n::{t, t_args};
use crate::keyboard::add_layout_dialog;
use crate::keyboard::keymap_grid::KeymapGridWidget;
use crate::keyboard::rename_dialog;
use crate::keyboard::variant_dialog;
use crate::keyboard::xkb_database;
use crate::keyboard::xkb_keymap;
use crate::search_index::SearchIndex;

/// Smart container for the Keyboard settings page.
pub struct KeyboardPage {
    pub root: gtk::Box,
}

/// Internal mutable state.
struct KeyboardPageState {
    ordered_list: OrderedList,
    empty_state: adw::StatusPage,
    add_button: gtk::Button,
    mode_banner: adw::Banner,
    keymap_widget: KeymapGridWidget,
    /// Ordered list of layout codes.
    layout_codes: Vec<String>,
    /// Custom names parallel to layout_codes (empty string = no custom name).
    layout_names: Vec<String>,
    /// Per-layout variant slots, parallel to layout_codes.
    variant_slots: Vec<String>,
    /// Current entity URN (set on first reconcile).
    config_urn: Option<Urn>,
    /// URN for the keyboard-layout entity (for set-active action).
    layout_urn: Option<Urn>,
    /// Currently active layout abbreviation (e.g., "US", "CZ").
    active_layout: Option<String>,
    /// Radio buttons parallel to layout_codes.
    radio_buttons: Vec<gtk::CheckButton>,
    /// Whether rename is supported (external-file mode).
    can_rename: bool,
}

impl KeyboardPage {
    pub fn new(
        entity_store: &Rc<EntityStore>,
        action_callback: &EntityActionCallback,
        search_index: &Rc<RefCell<SearchIndex>>,
    ) -> Self {
        let root = crate::page_layout::page_root();

        // Mode banner (hidden by default)
        let mode_banner = adw::Banner::builder().revealed(false).build();
        root.append(&mode_banner);

        // Layout list group
        let list_group = adw::PreferencesGroup::builder()
            .title(t("kb-layouts-title"))
            .description(t("kb-layouts-desc"))
            .build();

        // Create OrderedList instead of manual ListBox
        let ordered_list = OrderedList::new(OrderedListProps {
            css_classes: vec!["boxed-list".to_string()],
        });
        ordered_list.root.set_visible(false);
        list_group.add(&ordered_list.root);

        // Empty state shown when no layouts are configured
        let empty_state = adw::StatusPage::builder()
            .icon_name("input-keyboard-symbolic")
            .title(t("kb-no-layouts"))
            .description(t("kb-no-layouts-desc"))
            .build();
        list_group.add(&empty_state);

        root.append(&list_group);

        // Register search entries
        {
            let mut idx = search_index.borrow_mut();
            let page_title = t("settings-keyboard");
            idx.add_section("keyboard", &page_title, &t("kb-layouts-title"), "kb-layouts-title", &list_group);
            idx.add_input("keyboard", &page_title, &t("kb-layouts-title"), &t("kb-add-layout"), "kb-add-layout", &list_group);
        }

        // Add layout button
        let add_button = gtk::Button::builder()
            .label(t("kb-add-layout"))
            .halign(gtk::Align::Start)
            .build();
        root.append(&add_button);

        // Keyboard layout visualization
        let keymap_group = adw::PreferencesGroup::builder()
            .title(t("kb-active-layout"))
            .build();
        let keymap_widget = KeymapGridWidget::new();
        keymap_widget.set_visible(false);
        keymap_group.add(&keymap_widget.root);
        root.append(&keymap_group);

        let state = Rc::new(RefCell::new(KeyboardPageState {
            ordered_list: ordered_list.clone(),
            empty_state,
            add_button: add_button.clone(),
            mode_banner,
            keymap_widget,
            layout_codes: Vec::new(),
            layout_names: Vec::new(),
            variant_slots: Vec::new(),
            config_urn: None,
            layout_urn: None,
            active_layout: None,
            radio_buttons: Vec::new(),
            can_rename: false,
        }));

        // Connect OrderedList reorder output
        {
            let state_ref = state.clone();
            let cb_ref = action_callback.clone();

            ordered_list.connect_output(move |output| {
                let OrderedListOutput::Reordered(layout_code, from_index, to_index) = output;
                log::debug!(
                    "[keyboard-page] Layout '{}' reordered from {} to {}",
                    layout_code,
                    from_index,
                    to_index
                );

                // Compute new order
                let new_order = {
                    let s = state_ref.borrow();
                    let mut new_order = s.layout_codes.clone();
                    let item = new_order.remove(from_index);
                    new_order.insert(to_index, item);
                    new_order
                };

                // Send reorder action
                let urn = state_ref.borrow().config_urn.clone();
                if let Some(urn) = urn {
                    let params = serde_json::json!({ "layouts": new_order });
                    cb_ref(urn, "reorder".to_string(), params);
                }
            });
        }

        // Connect add button
        {
            let state_ref = state.clone();
            let cb = action_callback.clone();
            let root_ref = root.clone();
            add_button.connect_clicked(move |_| {
                let urn = {
                    let s = state_ref.borrow();
                    s.config_urn.clone()
                };

                if let Some(urn) = urn {
                    let cb_clone = cb.clone();
                    let urn_clone = urn;
                    add_layout_dialog::show_add_layout_dialog(
                        &root_ref,
                        move |layout_code, layout_name| {
                            log::debug!(
                                "[keyboard-page] Adding layout: {} ({})",
                                layout_code,
                                layout_name
                            );
                            let params =
                                serde_json::json!({ "layout": layout_code, "name": layout_name });
                            cb_clone(urn_clone.clone(), "add".to_string(), params);
                        },
                    );
                }
            });
        }

        // Subscribe to both config and active layout changes
        crate::subscription::subscribe_dual_entities::<KeyboardLayoutConfig, KeyboardLayout, _>(
            entity_store,
            CONFIG_ENTITY_TYPE,
            KEYBOARD_ENTITY_TYPE,
            {
                let state_clone = state.clone();
                let cb = action_callback.clone();
                move |configs, layouts| {
                    if let Some((urn, config)) = configs.first() {
                        log::debug!(
                            "[keyboard-page] Reconciling config with mode: {}",
                            config.mode
                        );
                        Self::reconcile(&state_clone, urn, config, &cb);
                    }
                    if let Some((urn, layout)) = layouts.first() {
                        Self::reconcile_active_layout(&state_clone, urn, layout);
                    }
                }
            },
        );

        Self { root }
    }

    fn reconcile(
        state: &Rc<RefCell<KeyboardPageState>>,
        urn: &Urn,
        config: &KeyboardLayoutConfig,
        action_callback: &EntityActionCallback,
    ) {
        let mut s = state.borrow_mut();

        // Store URN for action callbacks
        s.config_urn = Some(urn.clone());

        let can_rename = config.mode == "external-file";
        s.can_rename = can_rename;

        // Update mode banner
        match config.mode.as_str() {
            "external-file" => {
                let msg = if let Some(path) = &config.file_path {
                    t_args("kb-mode-external-file", &[("path", path)])
                } else {
                    t("kb-mode-external-file-no-path")
                };
                s.mode_banner.set_title(&msg);
                s.mode_banner.set_revealed(true);
                s.add_button.set_sensitive(true);
            }
            "error" => {
                let msg = if let Some(error) = &config.error_message {
                    t_args("kb-mode-error", &[("error", error)])
                } else {
                    t("kb-mode-error-no-detail")
                };
                s.mode_banner.set_title(&msg);
                s.mode_banner.set_revealed(true);
                s.add_button.set_sensitive(false);
            }
            "system-default" => {
                s.mode_banner
                    .set_title(&t("kb-mode-system-default"));
                s.mode_banner.set_revealed(true);
                s.add_button.set_sensitive(true);
            }
            "editable" => {
                s.mode_banner.set_revealed(false);
                s.add_button.set_sensitive(true);
            }
            _ => {
                s.mode_banner.set_revealed(false);
                s.add_button.set_sensitive(false);
            }
        }

        // Parse variant string into per-layout slots
        let variant_slots = parse_variant_slots(&config.variant, config.layouts.len());

        // Check if layout list or variants actually changed
        if s.layout_codes == config.layouts
            && s.layout_names == config.layout_names
            && s.variant_slots == variant_slots
        {
            return;
        }

        // Clear existing rows
        s.ordered_list.clear();
        s.layout_codes.clear();
        s.layout_names.clear();
        s.variant_slots.clear();
        s.radio_buttons.clear();

        // First radio button is the group leader
        let mut radio_group: Option<gtk::CheckButton> = None;

        // Add rows in order
        for (i, layout_code) in config.layouts.iter().enumerate() {
            let full_name = config
                .layout_names
                .iter()
                .zip(config.layouts.iter())
                .find(|(_, c)| *c == layout_code)
                .and_then(|(name, _)| {
                    if name.is_empty() {
                        None
                    } else {
                        Some(name.clone())
                    }
                })
                .unwrap_or_else(|| layout_code_to_name(layout_code));

            let current_variant = variant_slots.get(i).cloned().unwrap_or_default();

            // Build subtitle: layout code + variant if set
            let subtitle = if current_variant.is_empty() {
                layout_code.clone()
            } else {
                format!("{} ({})", layout_code, current_variant)
            };

            // Create OrderedListRow with native ActionRow layout
            let row = OrderedListRow::new(OrderedListRowProps {
                id: layout_code.clone(),
                draggable: true,
                title: full_name,
                subtitle: Some(subtitle),
            });

            // Radio button prefix for active layout selection
            let radio = gtk::CheckButton::builder()
                .valign(gtk::Align::Center)
                .build();

            // Group with first radio button
            if let Some(ref group_leader) = radio_group {
                radio.set_group(Some(group_leader));
            }
            if radio_group.is_none() {
                radio_group = Some(radio.clone());
            }

            // Active state will be set by reconcile_active_layout when
            // the keyboard-layout entity arrives
            radio.set_active(false);

            // Connect toggled signal to dispatch set-active action
            {
                let state_clone = state.clone();
                let cb_clone = action_callback.clone();
                let index = i;
                radio.connect_toggled(move |btn| {
                    if btn.is_active() {
                        let layout_urn = {
                            let s = state_clone.borrow();
                            s.layout_urn.clone()
                        };
                        if let Some(urn) = layout_urn {
                            log::debug!(
                                "[keyboard-page] Switching to layout index {}",
                                index
                            );
                            let params = serde_json::json!({ "index": index });
                            cb_clone(urn, "set-active".to_string(), params);
                        }
                    }
                });
            }

            row.add_suffix(&radio);
            s.radio_buttons.push(radio);

            // Variant button (shown only if layout has available variants)
            let available_variants = xkb_database::get_variants_for_layout(layout_code);
            if !available_variants.is_empty() {
                let variant_label = if current_variant.is_empty() {
                    t("kb-variant-none")
                } else {
                    // Find description for current variant
                    available_variants
                        .iter()
                        .find(|v| v.code == current_variant)
                        .map(|v| v.description.clone())
                        .unwrap_or_else(|| current_variant.clone())
                };

                let variant_btn = gtk::Button::builder()
                    .label(variant_label)
                    .tooltip_text(t("kb-variant-button"))
                    .valign(gtk::Align::Center)
                    .css_classes(["flat"])
                    .build();

                let code_clone = layout_code.clone();
                let state_clone = state.clone();
                let cb_clone = action_callback.clone();
                let urn_clone = urn.clone();
                let list_widget = s.ordered_list.root.clone();

                variant_btn.connect_clicked(move |_| {
                    let current = {
                        let s = state_clone.borrow();
                        let idx = s.layout_codes.iter().position(|c| c == &code_clone);
                        idx.and_then(|i| s.variant_slots.get(i))
                            .cloned()
                            .unwrap_or_default()
                    };

                    let cb = cb_clone.clone();
                    let urn = urn_clone.clone();
                    let code_for_dialog = code_clone.clone();
                    let code_for_cb = code_clone.clone();
                    variant_dialog::show_variant_dialog(
                        &list_widget,
                        &code_for_dialog,
                        &current,
                        move |new_variant| {
                            log::debug!(
                                "[keyboard-page] Setting variant for '{}' to '{}'",
                                code_for_cb,
                                if new_variant.is_empty() {
                                    "(none)"
                                } else {
                                    &new_variant
                                }
                            );
                            let params =
                                serde_json::json!({ "layout": code_for_cb, "variant": new_variant });
                            cb(urn.clone(), "set-variant".to_string(), params);
                        },
                    );
                });

                row.add_suffix(&variant_btn);
            }

            // Rename button (only if can_rename)
            if can_rename {
                let rename_btn = gtk::Button::builder()
                    .icon_name("document-edit-symbolic")
                    .tooltip_text(t("kb-rename-layout"))
                    .valign(gtk::Align::Center)
                    .css_classes(["flat"])
                    .build();

                let code_clone = layout_code.clone();
                let state_clone = state.clone();
                let cb_clone = action_callback.clone();
                let urn_clone = urn.clone();
                let list_widget = s.ordered_list.root.clone();

                rename_btn.connect_clicked(move |_| {
                    let current_name = {
                        let s = state_clone.borrow();
                        let idx = s.layout_codes.iter().position(|c| c == &code_clone);
                        idx.and_then(|i| s.layout_names.get(i))
                            .filter(|n| !n.is_empty())
                            .cloned()
                            .unwrap_or_else(|| layout_code_to_name(&code_clone))
                    };

                    let cb = cb_clone.clone();
                    let urn = urn_clone.clone();
                    let code = code_clone.clone();
                    rename_dialog::show_rename_dialog(
                        &list_widget,
                        &current_name,
                        move |new_name| {
                            log::debug!(
                                "[keyboard-page] Renaming layout '{}' to '{}'",
                                code,
                                new_name
                            );
                            let params = serde_json::json!({ "layout": code, "name": new_name });
                            cb(urn.clone(), "rename".to_string(), params);
                        },
                    );
                });

                row.add_suffix(&rename_btn);
            }

            // Remove button
            let remove_btn = gtk::Button::builder()
                .icon_name("user-trash-symbolic")
                .tooltip_text(t("kb-remove-layout"))
                .valign(gtk::Align::Center)
                .css_classes(["flat"])
                .build();

            let code_clone = layout_code.clone();
            let cb_clone = action_callback.clone();
            let urn_clone = urn.clone();
            remove_btn.connect_clicked(move |_| {
                log::debug!("[keyboard-page] Removing layout: {}", code_clone);
                let params = serde_json::json!({ "layout": code_clone });
                cb_clone(urn_clone.clone(), "remove".to_string(), params);
            });

            row.add_suffix(&remove_btn);

            s.ordered_list.append_item(&row);
            s.layout_codes.push(layout_code.clone());
        }

        // Store names and variants for lookups
        s.layout_names = config.layout_names.clone();
        s.variant_slots = variant_slots;

        // Toggle empty state vs list visibility
        let has_layouts = !s.layout_codes.is_empty();
        s.ordered_list.root.set_visible(has_layouts);
        s.empty_state.set_visible(!has_layouts);
    }

    /// Reconcile the active keyboard layout state.
    ///
    /// Updates radio button selection and keymap visualization based on
    /// the current active layout from the keyboard-layout entity.
    fn reconcile_active_layout(
        state: &Rc<RefCell<KeyboardPageState>>,
        urn: &Urn,
        layout: &KeyboardLayout,
    ) {
        // Find which index is active before mutating state
        let active_index = layout
            .available
            .iter()
            .position(|abbr| abbr == &layout.current);

        // Collect widget handles and keymap data while holding the borrow,
        // then drop it before calling set_active() — which fires connect_toggled
        // synchronously and would re-enter state.borrow(), causing a panic.
        let (radio_buttons, keymap_widget, keymap_code, keymap_variant) = {
            let mut s = state.borrow_mut();
            s.layout_urn = Some(urn.clone());
            s.active_layout = Some(layout.current.clone());

            let radios = s.radio_buttons.clone();
            let keymap_widget = s.keymap_widget.clone();
            let keymap_code = active_index.and_then(|i| s.layout_codes.get(i).cloned());
            let keymap_variant = active_index
                .and_then(|i| s.variant_slots.get(i).cloned())
                .unwrap_or_default();
            (radios, keymap_widget, keymap_code, keymap_variant)
        }; // borrow_mut dropped here — safe to fire GTK signals

        // Update radio buttons (may fire connect_toggled, which borrows state immutably)
        for (i, radio) in radio_buttons.iter().enumerate() {
            let should_be_active = active_index == Some(i);
            if radio.is_active() != should_be_active {
                radio.set_active(should_be_active);
            }
        }

        // Update keymap visualization
        if let Some(code) = keymap_code {
            if let Some(grid) = xkb_keymap::load_keymap_grid(&code, &keymap_variant) {
                keymap_widget.set_keymap(&grid);
                keymap_widget.set_visible(true);
            } else {
                log::debug!(
                    "[keyboard-page] No keymap grid for layout '{}' variant '{}'",
                    code,
                    keymap_variant
                );
                keymap_widget.set_visible(false);
            }
        } else {
            keymap_widget.set_visible(false);
        }
    }
}

/// Parse a variant string into per-layout slots.
///
/// The variant field is a comma-separated string parallel to layouts.
/// E.g., for `layouts: ["us", "cz"]`, `variant: Some(",qwerty")` means
/// `us` has no variant and `cz` has `qwerty`.
fn parse_variant_slots(variant: &Option<String>, layout_count: usize) -> Vec<String> {
    match variant {
        Some(v) if !v.is_empty() => {
            let mut slots: Vec<String> = v.split(',').map(|s| s.to_string()).collect();
            // Pad with empty strings if needed
            while slots.len() < layout_count {
                slots.push(String::new());
            }
            slots.truncate(layout_count);
            slots
        }
        _ => vec![String::new(); layout_count],
    }
}

/// Map a layout code to a human-readable name.
fn layout_code_to_name(code: &str) -> String {
    match code {
        "us" => "English (US)".to_string(),
        "gb" => "English (UK)".to_string(),
        "de" => "German".to_string(),
        "fr" => "French".to_string(),
        "cz" => "Czech".to_string(),
        "es" => "Spanish".to_string(),
        "it" => "Italian".to_string(),
        "pt" => "Portuguese".to_string(),
        "pl" => "Polish".to_string(),
        "ru" => "Russian".to_string(),
        "ua" => "Ukrainian".to_string(),
        "nl" => "Dutch".to_string(),
        "se" => "Swedish".to_string(),
        "no" => "Norwegian".to_string(),
        "dk" => "Danish".to_string(),
        "fi" => "Finnish".to_string(),
        "hu" => "Hungarian".to_string(),
        "sk" => "Slovak".to_string(),
        "ro" => "Romanian".to_string(),
        "bg" => "Bulgarian".to_string(),
        "hr" => "Croatian".to_string(),
        "rs" => "Serbian".to_string(),
        "si" => "Slovenian".to_string(),
        "gr" => "Greek".to_string(),
        "tr" => "Turkish".to_string(),
        "il" => "Hebrew".to_string(),
        "ara" => "Arabic".to_string(),
        "jp" => "Japanese".to_string(),
        "kr" => "Korean".to_string(),
        "cn" => "Chinese".to_string(),
        _ => code.to_uppercase(),
    }
}
