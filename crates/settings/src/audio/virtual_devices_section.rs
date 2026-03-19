//! Virtual devices section -- smart container.
//!
//! Subscribes to `audio-device` entity type, filters for `virtual_device == true`,
//! and renders per-device rows with volume sliders, mute buttons, and delete buttons.
//! Provides a single create button that opens a dialog with type selector.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use adw::prelude::*;
use gtk::glib;
use waft_client::EntityActionCallback;
use waft_protocol::Urn;
use waft_protocol::entity::audio::{AudioDevice, AudioDeviceKind};
use waft_ui_gtk::audio::icon::audio_device_icon;
use waft_ui_gtk::icons::IconWidget;

use crate::i18n::t;

/// Cancel any pending debounce and schedule a new one-shot timer.
///
/// When the timer fires, `interacting` is cleared and any stashed
/// `pending_value` is applied to the scale (blocking the signal handler to
/// prevent a spurious `value-changed` event).
fn schedule_interaction_end(
    debounce_source: &Rc<RefCell<Option<glib::SourceId>>>,
    interacting: &Rc<RefCell<bool>>,
    pending_value: &Rc<RefCell<Option<f64>>>,
    scale: &gtk::Scale,
    handler_id: &Rc<glib::SignalHandlerId>,
    delay_ms: u64,
) {
    if let Some(source_id) = debounce_source.borrow_mut().take() {
        source_id.remove();
    }

    let interacting = interacting.clone();
    let pending_value = pending_value.clone();
    let scale = scale.clone();
    let handler_id = handler_id.clone();
    let debounce_inner = debounce_source.clone();

    let source_id = glib::timeout_add_local_once(
        std::time::Duration::from_millis(delay_ms),
        move || {
            *debounce_inner.borrow_mut() = None;
            *interacting.borrow_mut() = false;

            if let Some(v) = pending_value.borrow_mut().take() {
                scale.block_signal(&handler_id);
                scale.set_value(v);
                scale.unblock_signal(&handler_id);
            }
        },
    );

    *debounce_source.borrow_mut() = Some(source_id);
}

/// Smart container for managing virtual audio devices.
#[derive(Clone)]
pub struct VirtualDevicesSection {
    pub root: gtk::Box,
    state: Rc<RefCell<VirtualDevicesSectionState>>,
}

struct VirtualDevicesSectionState {
    rows: HashMap<String, VirtualDeviceRow>,
    group: adw::PreferencesGroup,
    action_callback: EntityActionCallback,
}

struct VirtualDeviceRow {
    root: gtk::Box,
    #[allow(dead_code)]
    row: adw::ActionRow,
    slider: gtk::Scale,
    slider_handler_id: Rc<glib::SignalHandlerId>,
    interacting: Rc<RefCell<bool>>,
    pending_value: Rc<RefCell<Option<f64>>>,
    #[allow(dead_code)]
    pointer_down: Rc<RefCell<bool>>,
    #[allow(dead_code)]
    debounce_source: Rc<RefCell<Option<glib::SourceId>>>,
    mute_button: gtk::Button,
}

impl VirtualDeviceRow {
    /// Set the slider volume, respecting active interaction state.
    fn set_volume(&self, v: f64) {
        if *self.interacting.borrow() {
            *self.pending_value.borrow_mut() = Some(v);
            return;
        }
        self.slider.block_signal(&self.slider_handler_id);
        self.slider.set_value(v);
        self.slider.unblock_signal(&self.slider_handler_id);
    }
}

impl VirtualDevicesSection {
    pub fn new(action_callback: &EntityActionCallback) -> Self {
        let root = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .build();

        let group = adw::PreferencesGroup::builder()
            .title(t("audio-virtual-devices"))
            .build();

        // Single create button
        let create_button = gtk::Button::builder()
            .label(t("audio-add-virtual-device"))
            .icon_name("list-add-symbolic")
            .css_classes(["flat"])
            .valign(gtk::Align::Center)
            .build();

        group.set_header_suffix(Some(&create_button));

        // Set description as empty state hint (shown when no virtual devices exist)
        group.set_description(Some(&t("audio-virtual-devices-empty-desc")));

        root.append(&group);

        let state = Rc::new(RefCell::new(VirtualDevicesSectionState {
            rows: HashMap::new(),
            group: group.clone(),
            action_callback: action_callback.clone(),
        }));

        // Wire create button
        {
            let cb = action_callback.clone();
            create_button.connect_clicked(move |btn| {
                let cb = cb.clone();
                show_create_dialog(
                    btn.root().and_downcast_ref::<gtk::Window>(),
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

            if let Some(existing) = state.rows.get(&urn_str) {
                // Update volume (respecting interaction guard)
                existing.set_volume(device.volume);

                // Update mute button icon/tooltip
                let mute_icon = if device.muted {
                    "audio-volume-muted-symbolic"
                } else {
                    "audio-volume-high-symbolic"
                };
                let mute_label = if device.muted {
                    t("audio-card-unmute")
                } else {
                    t("audio-card-mute")
                };
                existing.mute_button.set_icon_name(mute_icon);
                existing.mute_button.set_tooltip_text(Some(&mute_label));

                // Update kind subtitle
                let kind_label = match device.kind {
                    AudioDeviceKind::Output => t("audio-output-devices"),
                    AudioDeviceKind::Input => t("audio-input-devices"),
                };
                existing.row.set_subtitle(&kind_label);
                existing.row.set_title(&device.name);
                continue;
            }

            let row = self.build_virtual_row(urn, device, &state.action_callback);
            state.group.add(&row.root);
            state.rows.insert(urn_str, row);
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
                state.group.remove(&vd_row.root);
            }
        }

        // Show/hide empty state description
        if virtual_devices.is_empty() {
            state
                .group
                .set_description(Some(&t("audio-virtual-devices-empty-desc")));
        } else {
            state.group.set_description(None);
        }
    }

    fn build_virtual_row(
        &self,
        urn: &Urn,
        device: &AudioDevice,
        action_callback: &EntityActionCallback,
    ) -> VirtualDeviceRow {
        let root = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .spacing(0)
            .build();

        let icon = IconWidget::from_name(
            audio_device_icon("virtual", device.kind),
            16,
        );

        // Info row with icon, name, mute button, delete button
        let info_row = adw::ActionRow::builder()
            .title(&device.name)
            .activatable(false)
            .build();
        info_row.add_prefix(icon.widget());

        // Kind indicator subtitle
        let kind_label = match device.kind {
            AudioDeviceKind::Output => t("audio-output-devices"),
            AudioDeviceKind::Input => t("audio-input-devices"),
        };
        info_row.set_subtitle(&kind_label);

        // Delete button
        let delete_button = gtk::Button::builder()
            .icon_name("user-trash-symbolic")
            .tooltip_text(t("audio-remove-virtual-device"))
            .css_classes(["flat", "destructive-action"])
            .valign(gtk::Align::Center)
            .build();

        let sink_name = device
            .sink_name
            .clone()
            .unwrap_or_else(|| urn.id().to_string());

        {
            let cb = action_callback.clone();
            let device_urn = urn.clone();
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
        }

        // Mute button
        let mute_icon = if device.muted {
            "audio-volume-muted-symbolic"
        } else {
            "audio-volume-high-symbolic"
        };
        let mute_label = if device.muted {
            t("audio-card-unmute")
        } else {
            t("audio-card-mute")
        };
        let mute_button = gtk::Button::builder()
            .icon_name(mute_icon)
            .valign(gtk::Align::Center)
            .css_classes(["flat"])
            .tooltip_text(&mute_label)
            .build();

        {
            let cb = action_callback.clone();
            let device_urn = urn.clone();
            mute_button.connect_clicked(move |_| {
                cb(
                    device_urn.clone(),
                    "toggle-mute".to_string(),
                    serde_json::json!({}),
                );
            });
        }

        info_row.add_suffix(&delete_button);
        info_row.add_suffix(&mute_button);
        root.append(&info_row);

        // Volume slider
        let slider_row = gtk::Box::builder()
            .orientation(gtk::Orientation::Horizontal)
            .spacing(8)
            .margin_start(48)
            .margin_end(12)
            .margin_bottom(4)
            .build();

        let slider = gtk::Scale::builder()
            .orientation(gtk::Orientation::Horizontal)
            .hexpand(true)
            .valign(gtk::Align::Center)
            .build();
        slider.set_range(0.0, 1.0);
        slider.set_value(device.volume);
        slider.set_draw_value(false);

        // Wrap for gesture tracking
        let scale_wrapper = gtk::Box::new(gtk::Orientation::Horizontal, 0);
        scale_wrapper.set_hexpand(true);
        scale_wrapper.append(&slider);

        slider_row.append(&scale_wrapper);
        root.append(&slider_row);

        // Interaction tracking state
        let interacting: Rc<RefCell<bool>> = Rc::new(RefCell::new(false));
        let pointer_down: Rc<RefCell<bool>> = Rc::new(RefCell::new(false));
        let pending_value: Rc<RefCell<Option<f64>>> = Rc::new(RefCell::new(None));
        let debounce_source: Rc<RefCell<Option<glib::SourceId>>> = Rc::new(RefCell::new(None));

        // Wire value_changed
        let cb = action_callback.clone();
        let device_urn = urn.clone();
        let interacting_vc = interacting.clone();
        let pointer_down_vc = pointer_down.clone();
        let debounce_source_vc = debounce_source.clone();
        let pending_vc = pending_value.clone();
        let slider_vc = slider.clone();

        let handler_id_holder: Rc<RefCell<Option<glib::SignalHandlerId>>> =
            Rc::new(RefCell::new(None));
        let handler_id_holder_vc = handler_id_holder.clone();

        let raw_handler_id = slider.connect_value_changed(move |scale| {
            // Keyboard/scroll interaction path
            if !*pointer_down_vc.borrow() {
                *interacting_vc.borrow_mut() = true;
                if let Some(src) = debounce_source_vc.borrow_mut().take() {
                    src.remove();
                }
                let interacting_d = interacting_vc.clone();
                let pending_d = pending_vc.clone();
                let slider_d = slider_vc.clone();
                let debounce_inner = debounce_source_vc.clone();
                let holder_d = handler_id_holder_vc.clone();
                let src = glib::timeout_add_local_once(
                    std::time::Duration::from_millis(200),
                    move || {
                        *debounce_inner.borrow_mut() = None;
                        *interacting_d.borrow_mut() = false;
                        if let Some(v) = pending_d.borrow_mut().take()
                            && let Some(ref hid) = *holder_d.borrow()
                        {
                            slider_d.block_signal(hid);
                            slider_d.set_value(v);
                            slider_d.unblock_signal(hid);
                        }
                    },
                );
                *debounce_source_vc.borrow_mut() = Some(src);
            }
            cb(
                device_urn.clone(),
                "set-volume".to_string(),
                serde_json::json!({ "value": scale.value() }),
            );
        });

        *handler_id_holder.borrow_mut() = Some(raw_handler_id);

        let slider_handler_id: Rc<glib::SignalHandlerId> = Rc::new(
            handler_id_holder
                .borrow_mut()
                .take()
                .expect("handler_id must be stored"),
        );

        // GestureClick on the wrapper box
        let gesture = gtk::GestureClick::new();

        let interacting_pressed = interacting.clone();
        let pointer_down_pressed = pointer_down.clone();
        let debounce_pressed = debounce_source.clone();
        gesture.connect_pressed(move |_, _, _, _| {
            *pointer_down_pressed.borrow_mut() = true;
            *interacting_pressed.borrow_mut() = true;
            if let Some(src) = debounce_pressed.borrow_mut().take() {
                src.remove();
            }
        });

        let interacting_released = interacting.clone();
        let pointer_down_released = pointer_down.clone();
        let pending_released = pending_value.clone();
        let debounce_released = debounce_source.clone();
        let slider_released = slider.clone();
        let handler_released = slider_handler_id.clone();
        gesture.connect_released(move |_, _, _, _| {
            *pointer_down_released.borrow_mut() = false;
            schedule_interaction_end(
                &debounce_released,
                &interacting_released,
                &pending_released,
                &slider_released,
                &handler_released,
                100,
            );
        });

        let interacting_cancel = interacting.clone();
        let pointer_down_cancel = pointer_down.clone();
        let pending_cancel = pending_value.clone();
        let debounce_cancel = debounce_source.clone();
        let slider_cancel = slider.clone();
        let handler_cancel = slider_handler_id.clone();
        gesture.connect_cancel(move |_, _| {
            *pointer_down_cancel.borrow_mut() = false;
            schedule_interaction_end(
                &debounce_cancel,
                &interacting_cancel,
                &pending_cancel,
                &slider_cancel,
                &handler_cancel,
                100,
            );
        });

        scale_wrapper.add_controller(gesture);

        VirtualDeviceRow {
            root,
            row: info_row,
            slider,
            slider_handler_id,
            interacting,
            pending_value,
            pointer_down,
            debounce_source,
            mute_button,
        }
    }
}

/// Show a dialog to create a virtual device with type selector.
fn show_create_dialog(
    parent: Option<&gtk::Window>,
    action_callback: &EntityActionCallback,
) {
    let dialog = adw::AlertDialog::builder()
        .heading(t("audio-create-device-title"))
        .close_response("cancel")
        .default_response("create")
        .build();

    dialog.add_response("cancel", "Cancel");
    dialog.add_response("create", &t("audio-create-virtual-sink"));
    dialog.set_response_appearance("create", adw::ResponseAppearance::Suggested);

    // Type selector combo row
    let type_model = gtk::StringList::new(&[
        &t("audio-virtual-type-sink"),
        &t("audio-virtual-type-source"),
    ]);
    let type_combo = adw::ComboRow::builder()
        .title(t("audio-virtual-device-type"))
        .model(&type_model)
        .build();

    // Update create button label when type selection changes
    {
        let dialog_ref = dialog.clone();
        type_combo.connect_selected_notify(move |combo| {
            let label = if combo.selected() == 0 {
                t("audio-create-virtual-sink")
            } else {
                t("audio-create-virtual-source")
            };
            dialog_ref.set_response_label("create", &label);
        });
    }

    let entry = adw::EntryRow::builder()
        .title(t("audio-create-device-label"))
        .build();
    entry.set_text(&t("audio-create-device-label-placeholder"));
    entry.select_region(0, -1);

    let list_box = gtk::ListBox::builder()
        .selection_mode(gtk::SelectionMode::None)
        .css_classes(["boxed-list"])
        .build();
    list_box.append(&type_combo);
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
    let entry_clone = entry.clone();
    let type_combo_clone = type_combo.clone();
    dialog.connect_response(None, move |_, response| {
        if response != "create" {
            return;
        }
        let label = entry_clone.text().to_string();
        if label.is_empty() {
            return;
        }

        let synthetic_urn = Urn::new("audio", "audio-device", "virtual");
        let action = if type_combo_clone.selected() == 0 {
            "create-sink"
        } else {
            "create-source"
        };
        let params = serde_json::json!({ "label": label });
        cb(synthetic_urn, action.to_string(), params);
    });

    dialog.present(parent);
}
