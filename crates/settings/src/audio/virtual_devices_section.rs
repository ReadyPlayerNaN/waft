//! Virtual devices section -- smart container.
//!
//! Subscribes to `audio-device` entity type, filters for `virtual_device == true`,
//! and renders per-device rows with delete buttons. Provides create buttons for
//! null-sink and null-source virtual devices.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use adw::prelude::*;
use waft_client::EntityActionCallback;
use waft_protocol::Urn;
use waft_protocol::entity::audio::{AudioDevice, AudioDeviceKind};

use crate::i18n::t;

/// Smart container for managing virtual audio devices.
#[derive(Clone)]
pub struct VirtualDevicesSection {
    pub root: gtk::Box,
    state: Rc<RefCell<VirtualDevicesSectionState>>,
}

struct VirtualDevicesSectionState {
    rows: HashMap<String, VirtualDeviceRow>,
    group: adw::PreferencesGroup,
    empty_state: adw::StatusPage,
    action_callback: EntityActionCallback,
}

struct VirtualDeviceRow {
    row: adw::ActionRow,
}

impl VirtualDevicesSection {
    pub fn new(action_callback: &EntityActionCallback) -> Self {
        let root = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .build();

        let group = adw::PreferencesGroup::builder()
            .title(t("audio-virtual-devices"))
            .build();

        // Header suffix with create buttons
        let button_box = gtk::Box::builder()
            .orientation(gtk::Orientation::Horizontal)
            .spacing(6)
            .valign(gtk::Align::Center)
            .build();

        let create_sink_button = gtk::Button::builder()
            .label(t("audio-create-virtual-sink"))
            .css_classes(["flat"])
            .build();

        let create_source_button = gtk::Button::builder()
            .label(t("audio-create-virtual-source"))
            .css_classes(["flat"])
            .build();

        button_box.append(&create_sink_button);
        button_box.append(&create_source_button);
        group.set_header_suffix(Some(&button_box));

        // Empty state
        let empty_state = adw::StatusPage::builder()
            .icon_name("audio-card-symbolic")
            .title(t("audio-virtual-devices-empty"))
            .description(t("audio-virtual-devices-empty-desc"))
            .build();

        root.append(&group);
        root.append(&empty_state);

        let state = Rc::new(RefCell::new(VirtualDevicesSectionState {
            rows: HashMap::new(),
            group: group.clone(),
            empty_state,
            action_callback: action_callback.clone(),
        }));

        // Wire create sink button
        {
            let cb = action_callback.clone();
            create_sink_button.connect_clicked(move |btn| {
                let cb = cb.clone();
                show_create_dialog(
                    btn.root().and_downcast_ref::<gtk::Window>(),
                    "null-sink",
                    &cb,
                );
            });
        }

        // Wire create source button
        {
            let cb = action_callback.clone();
            create_source_button.connect_clicked(move |btn| {
                let cb = cb.clone();
                show_create_dialog(
                    btn.root().and_downcast_ref::<gtk::Window>(),
                    "null-source",
                    &cb,
                );
            });
        }

        Self { root, state }
    }

    /// Reconcile virtual device rows from entity data.
    pub fn reconcile(&self, devices: &[(Urn, AudioDevice)]) {
        let virtual_devices: Vec<_> = devices
            .iter()
            .filter(|(_, d)| d.virtual_device)
            .collect();

        let mut state = self.state.borrow_mut();
        let mut seen = std::collections::HashSet::new();

        for (urn, device) in &virtual_devices {
            let urn_str = urn.as_str().to_string();
            seen.insert(urn_str.clone());

            if state.rows.contains_key(&urn_str) {
                // Existing row -- nothing to update for virtual devices
                continue;
            }

            let sink_name = device
                .sink_name
                .clone()
                .unwrap_or_else(|| urn.id().to_string());

            let row = adw::ActionRow::builder()
                .title(&device.name)
                .build();

            // Kind indicator subtitle
            let kind_label = match device.kind {
                AudioDeviceKind::Output => t("audio-output-devices"),
                AudioDeviceKind::Input => t("audio-input-devices"),
            };
            row.set_subtitle(&kind_label);

            // Delete button
            let delete_button = gtk::Button::builder()
                .icon_name("user-trash-symbolic")
                .tooltip_text(t("audio-remove-virtual-device"))
                .css_classes(["flat", "destructive-action"])
                .valign(gtk::Align::Center)
                .build();

            let cb = state.action_callback.clone();
            let device_urn = (*urn).clone();
            let device_kind = device.kind;
            let device_sink_name = sink_name.clone();
            delete_button.connect_clicked(move |_| {
                let (action, params) = match device_kind {
                    AudioDeviceKind::Output => (
                        "remove-sink",
                        serde_json::json!({ "sink_name": device_sink_name }),
                    ),
                    AudioDeviceKind::Input => (
                        "remove-source",
                        serde_json::json!({ "source_name": device_sink_name }),
                    ),
                };
                cb(device_urn.clone(), action.to_string(), params);
            });

            row.add_suffix(&delete_button);
            state.group.add(&row);
            state.rows.insert(urn_str, VirtualDeviceRow { row });
        }

        // Remove rows no longer present
        let to_remove: Vec<String> = state
            .rows
            .keys()
            .filter(|k| !seen.contains(*k))
            .cloned()
            .collect();

        for key in to_remove {
            if let Some(vd_row) = state.rows.remove(&key) {
                state.group.remove(&vd_row.row);
            }
        }

        // Show/hide empty state
        let has_devices = !virtual_devices.is_empty();
        state.group.set_visible(has_devices);
        state.empty_state.set_visible(!has_devices);
    }
}

/// Show a dialog to create a virtual device.
fn show_create_dialog(
    parent: Option<&gtk::Window>,
    module_type: &str,
    action_callback: &EntityActionCallback,
) {
    let dialog = adw::AlertDialog::builder()
        .heading(t("audio-create-device-title"))
        .close_response("cancel")
        .default_response("create")
        .build();

    dialog.add_response("cancel", &t("audio-remove-virtual-device"));
    dialog.add_response("create", &t("audio-create-virtual-sink"));

    // Use OK/Cancel style -- the response label for "create" should be contextual
    let action_label = match module_type {
        "null-sink" => t("audio-create-virtual-sink"),
        _ => t("audio-create-virtual-source"),
    };
    dialog.set_response_label("create", &action_label);
    dialog.set_response_label("cancel", "Cancel");
    dialog.set_response_appearance("create", adw::ResponseAppearance::Suggested);

    let entry = adw::EntryRow::builder()
        .title(t("audio-create-device-label"))
        .build();
    entry.set_text(&t("audio-create-device-label-placeholder"));
    entry.select_region(0, -1);

    let list_box = gtk::ListBox::builder()
        .selection_mode(gtk::SelectionMode::None)
        .css_classes(["boxed-list"])
        .build();
    list_box.append(&entry);

    dialog.set_extra_child(Some(&list_box));

    // Disable create when entry is empty
    dialog.set_response_enabled("create", !entry.text().is_empty());
    {
        let dialog_ref = dialog.clone();
        entry.connect_changed(move |e| {
            dialog_ref.set_response_enabled("create", !e.text().is_empty());
        });
    }

    let cb = action_callback.clone();
    let module_type = module_type.to_string();
    let entry_clone = entry.clone();
    dialog.connect_response(None, move |_, response| {
        if response != "create" {
            return;
        }
        let label = entry_clone.text().to_string();
        if label.is_empty() {
            return;
        }

        let synthetic_urn = Urn::new("audio", "audio-device", "virtual");
        let action = match module_type.as_str() {
            "null-sink" => "create-sink",
            _ => "create-source",
        };
        let params = serde_json::json!({ "label": label });
        cb(synthetic_urn, action.to_string(), params);
    });

    dialog.present(parent);
}
