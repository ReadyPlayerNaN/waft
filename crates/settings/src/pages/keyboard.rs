//! Keyboard settings page -- smart container.
//!
//! Subscribes to `EntityStore` for `keyboard-layout-config` entity type.
//! On entity changes, reconciles keyboard layout list with drag-and-drop support.

use std::cell::RefCell;
use std::rc::Rc;

use adw::prelude::*;
use waft_client::{EntityActionCallback, EntityStore};
use waft_protocol::Urn;
use waft_protocol::entity::keyboard::{CONFIG_ENTITY_TYPE, KeyboardLayoutConfig};
use waft_ui_gtk::widgets::ordered_list::{OrderedList, OrderedListOutput, OrderedListProps};
use waft_ui_gtk::widgets::ordered_list_row::{OrderedListRow, OrderedListRowProps};

use crate::keyboard::add_layout_dialog;
use crate::keyboard::rename_dialog;

/// Smart container for the Keyboard settings page.
pub struct KeyboardPage {
    pub root: gtk::Box,
}

/// Internal mutable state.
struct KeyboardPageState {
    ordered_list: OrderedList,
    add_button: gtk::Button,
    mode_banner: adw::Banner,
    /// Ordered list of layout codes.
    layout_codes: Vec<String>,
    /// Custom names parallel to layout_codes (empty string = no custom name).
    layout_names: Vec<String>,
    /// Current entity URN (set on first reconcile).
    config_urn: Option<Urn>,
    /// Whether rename is supported (external-file mode).
    can_rename: bool,
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
            .description(
                "Configure available keyboard layouts for switching. Drag the handle to reorder.",
            )
            .build();

        // Create OrderedList instead of manual ListBox
        let ordered_list = OrderedList::new(OrderedListProps {
            css_classes: vec!["boxed-list".to_string()],
        });
        list_group.add(&ordered_list.root);

        root.append(&list_group);

        // Add layout button
        let add_button = gtk::Button::builder()
            .label("Add Layout")
            .halign(gtk::Align::Start)
            .build();
        root.append(&add_button);

        let state = Rc::new(RefCell::new(KeyboardPageState {
            ordered_list: ordered_list.clone(),
            add_button: add_button.clone(),
            mode_banner,
            layout_codes: Vec::new(),
            layout_names: Vec::new(),
            config_urn: None,
            can_rename: false,
        }));

        // Connect OrderedList reorder output
        {
            let state_ref = state.clone();
            let cb_ref = action_callback.clone();

            ordered_list.connect_output(move |output| {
                if let OrderedListOutput::Reordered(layout_code, from_index, to_index) = output {
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

        let can_rename = config.mode == "external-file";
        s.can_rename = can_rename;

        // Update mode banner
        match config.mode.as_str() {
            "external-file" => {
                let msg = if let Some(path) = &config.file_path {
                    format!(
                        "Layouts configured via XKB file: {}. Changes require restarting the Niri session.",
                        path
                    )
                } else {
                    "Layouts configured via XKB file. Changes require restarting the Niri session."
                        .to_string()
                };
                s.mode_banner.set_title(&msg);
                s.mode_banner.set_revealed(true);
                s.add_button.set_sensitive(true);
            }
            "error" => {
                let msg = if let Some(error) = &config.error_message {
                    format!(
                        "Configuration error: {}. Please check ~/.config/niri/config.kdl",
                        error
                    )
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
        if s.layout_codes == config.layouts && s.layout_names == config.layout_names {
            return;
        }

        // Clear existing rows
        s.ordered_list.clear();
        s.layout_codes.clear();
        s.layout_names.clear();

        // Add rows in order
        for layout_code in config.layouts.iter() {
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

            // Create OrderedListRow with native ActionRow layout
            let row = OrderedListRow::new(OrderedListRowProps {
                id: layout_code.clone(),
                draggable: true,
                title: full_name,
                subtitle: Some(layout_code.clone()),
            });

            // Rename button (only if can_rename)
            if can_rename {
                let rename_btn = gtk::Button::builder()
                    .icon_name("document-edit-symbolic")
                    .tooltip_text("Rename layout")
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
                .tooltip_text("Remove layout")
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

        // Store names for rename lookups
        s.layout_names = config.layout_names.clone();
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
