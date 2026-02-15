//! Keyboard settings page -- smart container.
//!
//! Subscribes to `EntityStore` for `keyboard-layout-config` entity type.
//! On entity changes, reconciles keyboard layout list.

use std::cell::RefCell;
use std::rc::Rc;

use adw::prelude::*;
use waft_client::{EntityActionCallback, EntityStore};
use waft_protocol::Urn;
use waft_protocol::entity::keyboard::{CONFIG_ENTITY_TYPE, KeyboardLayoutConfig};

use crate::keyboard::add_layout_dialog;
use crate::keyboard::layout_row::{LayoutRow, LayoutRowOutput};

/// Smart container for the Keyboard settings page.
pub struct KeyboardPage {
    pub root: gtk::Box,
}

/// Internal mutable state.
struct KeyboardPageState {
    layout_list: gtk::ListBox,
    add_button: gtk::Button,
    mode_banner: adw::Banner,
    /// Ordered list of layout row codes, matching the ListBox order.
    layout_codes: Vec<String>,
    /// Map from layout code to LayoutRow widget.
    layout_rows: std::collections::HashMap<String, LayoutRow>,
    /// Current entity URN (set on first reconcile).
    config_urn: Option<Urn>,
}

impl KeyboardPage {
    pub fn new(entity_store: &Rc<EntityStore>, action_callback: &EntityActionCallback) -> Self {
        let root = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .spacing(24)
            .margin_top(24)
            .margin_bottom(24)
            .margin_start(12)
            .margin_end(12)
            .build();

        // Mode banner (hidden by default)
        let mode_banner = adw::Banner::builder().revealed(false).build();
        root.append(&mode_banner);

        // Layout list group
        let list_group = adw::PreferencesGroup::builder()
            .title("Keyboard Layouts")
            .description("Configure available keyboard layouts for switching")
            .build();

        let layout_list = gtk::ListBox::builder()
            .selection_mode(gtk::SelectionMode::None)
            .css_classes(["boxed-list"])
            .build();
        list_group.add(&layout_list);

        root.append(&list_group);

        // Add layout button
        let add_button = gtk::Button::builder()
            .label("Add Layout")
            .css_classes(["suggested-action"])
            .halign(gtk::Align::Start)
            .build();
        root.append(&add_button);

        let state = Rc::new(RefCell::new(KeyboardPageState {
            layout_list,
            add_button: add_button.clone(),
            mode_banner,
            layout_codes: Vec::new(),
            layout_rows: std::collections::HashMap::new(),
            config_urn: None,
        }));

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
                        move |layout_code| {
                            log::debug!("[keyboard-page] Adding layout: {}", layout_code);
                            let params = serde_json::json!({ "layout": layout_code });
                            cb_clone(urn_clone.clone(), "add".to_string(), params);
                        },
                    );
                }
            });
        }

        // Subscribe to keyboard-layout-config changes
        {
            let store = entity_store.clone();
            let cb = action_callback.clone();
            let state_clone = state.clone();

            entity_store.subscribe_type(CONFIG_ENTITY_TYPE, move || {
                let configs: Vec<(Urn, KeyboardLayoutConfig)> =
                    store.get_entities_typed(CONFIG_ENTITY_TYPE);

                log::debug!(
                    "[keyboard-page] Config subscription triggered: {} configs",
                    configs.len()
                );

                if let Some((urn, config)) = configs.first() {
                    Self::reconcile(&state_clone, urn, config, &cb);
                }
            });
        }

        // Initial reconciliation
        {
            let state_clone = state.clone();
            let store_clone = entity_store.clone();
            let cb_clone = action_callback.clone();

            gtk::glib::idle_add_local_once(move || {
                let configs: Vec<(Urn, KeyboardLayoutConfig)> =
                    store_clone.get_entities_typed(CONFIG_ENTITY_TYPE);

                if let Some((urn, config)) = configs.first() {
                    log::debug!(
                        "[keyboard-page] Initial reconciliation with mode: {}",
                        config.mode
                    );
                    Self::reconcile(&state_clone, urn, config, &cb_clone);
                }
            });
        }

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

        // Update mode banner
        match config.mode.as_str() {
            "external-file" => {
                let msg = if let Some(path) = &config.file_path {
                    format!(
                        "Using custom XKB file: {}. Remove the 'file' option from niri config to configure layouts here.",
                        path
                    )
                } else {
                    "Using custom XKB file. Remove the 'file' option from niri config to configure layouts here.".to_string()
                };
                s.mode_banner.set_title(&msg);
                s.mode_banner.set_revealed(true);
                s.add_button.set_sensitive(false);
            }
            "error" => {
                let msg = if let Some(error) = &config.error_message {
                    format!("Configuration error: {}. Please check ~/.config/niri/config.kdl", error)
                } else {
                    "Configuration error. Please check ~/.config/niri/config.kdl".to_string()
                };
                s.mode_banner.set_title(&msg);
                s.mode_banner.set_revealed(true);
                s.add_button.set_sensitive(false);
            }
            "system-default" => {
                s.mode_banner
                    .set_title("Using system defaults. Add a layout to start configuring.");
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

        // Check if layout list actually changed
        if s.layout_codes == config.layouts {
            // Just update move button sensitivity (in case count didn't change but
            // this is a different reconcile trigger)
            Self::update_move_buttons(&s);
            return;
        }

        // Clear existing rows
        let old_codes: Vec<String> = s.layout_codes.drain(..).collect();
        for code in &old_codes {
            if let Some(row) = s.layout_rows.remove(code) {
                s.layout_list.remove(row.widget());
            }
        }

        // Add rows in order
        let layout_count = config.layouts.len();
        for (i, layout_code) in config.layouts.iter().enumerate() {
            let full_name = layout_code_to_name(layout_code);
            let row = LayoutRow::new(layout_code, &full_name);

            row.set_can_move_up(i > 0);
            row.set_can_move_down(i < layout_count - 1);

            // Connect output handlers
            let urn_clone = urn.clone();
            let cb_clone = action_callback.clone();
            let state_clone = state.clone();
            row.connect_output(move |output| match output {
                LayoutRowOutput::Remove(code) => {
                    log::debug!("[keyboard-page] Removing layout: {}", code);
                    let params = serde_json::json!({ "layout": code });
                    cb_clone(urn_clone.clone(), "remove".to_string(), params);
                }
                LayoutRowOutput::MoveUp(code) => {
                    let s = state_clone.borrow();
                    if let Some(new_order) = move_layout_up(&s.layout_codes, &code) {
                        log::debug!("[keyboard-page] Moving layout up: {}", code);
                        let params = serde_json::json!({ "layouts": new_order });
                        cb_clone(urn_clone.clone(), "reorder".to_string(), params);
                    }
                }
                LayoutRowOutput::MoveDown(code) => {
                    let s = state_clone.borrow();
                    if let Some(new_order) = move_layout_down(&s.layout_codes, &code) {
                        log::debug!("[keyboard-page] Moving layout down: {}", code);
                        let params = serde_json::json!({ "layouts": new_order });
                        cb_clone(urn_clone.clone(), "reorder".to_string(), params);
                    }
                }
            });

            s.layout_list.append(row.widget());
            s.layout_rows.insert(layout_code.clone(), row);
            s.layout_codes.push(layout_code.clone());
        }
    }

    fn update_move_buttons(s: &KeyboardPageState) {
        let count = s.layout_codes.len();
        for (i, code) in s.layout_codes.iter().enumerate() {
            if let Some(row) = s.layout_rows.get(code) {
                row.set_can_move_up(i > 0);
                row.set_can_move_down(i < count - 1);
            }
        }
    }
}

/// Move a layout code one position up in the list.
fn move_layout_up(layouts: &[String], code: &str) -> Option<Vec<String>> {
    let idx = layouts.iter().position(|c| c == code)?;
    if idx == 0 {
        return None;
    }
    let mut new_order = layouts.to_vec();
    new_order.swap(idx, idx - 1);
    Some(new_order)
}

/// Move a layout code one position down in the list.
fn move_layout_down(layouts: &[String], code: &str) -> Option<Vec<String>> {
    let idx = layouts.iter().position(|c| c == code)?;
    if idx >= layouts.len() - 1 {
        return None;
    }
    let mut new_order = layouts.to_vec();
    new_order.swap(idx, idx + 1);
    Some(new_order)
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
