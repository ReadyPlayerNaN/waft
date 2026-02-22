//! Per-card audio widget.
//!
//! Dumb widget displaying a physical audio card as an `adw::PreferencesGroup`
//! with profile selector, and per-sink/source rows with volume, mute, default,
//! and port controls.

use std::cell::RefCell;
use std::rc::Rc;

use adw::prelude::*;
use gtk::glib;
use waft_protocol::entity::audio::{
    AudioCard, AudioCardSink, AudioCardSource, AudioDeviceKind, AudioPort,
};
use waft_ui_gtk::audio::icon::audio_device_icon;
use waft_ui_gtk::icons::IconWidget;

use crate::i18n::{t, t_args};

/// Output events from an audio device card.
pub enum AudioDeviceCardOutput {
    SetProfile(String),
    SetSinkVolume { sink: String, volume: f64 },
    ToggleSinkMute { sink: String },
    SetSinkDefault { sink: String },
    SetSinkPort { sink: String, port: String },
    SetSourceVolume { source: String, volume: f64 },
    ToggleSourceMute { source: String },
    SetSourceDefault { source: String },
    SetSourcePort { source: String, port: String },
}

/// Callback type for card output events.
type OutputCallback = Rc<RefCell<Option<Box<dyn Fn(AudioDeviceCardOutput)>>>>;

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
    // Cancel any existing debounce timer
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

/// A per-sink row widget with interaction tracking state.
struct SinkRow {
    root: gtk::Box,
    sink_name: String,
    slider: gtk::Scale,
    /// Signal handler ID stored in an `Rc` so it can be shared with closures
    /// that need to block/unblock the signal without requiring `Clone`.
    slider_handler_id: Rc<glib::SignalHandlerId>,
    interacting: Rc<RefCell<bool>>,
    pending_value: Rc<RefCell<Option<f64>>>,
    /// Tracks whether the pointer is physically held down.
    #[allow(dead_code)]
    pointer_down: Rc<RefCell<bool>>,
    /// Held to keep active debounce `SourceId` alive.
    #[allow(dead_code)]
    debounce_source: Rc<RefCell<Option<glib::SourceId>>>,
    /// The info row, kept for updating subtitle (default indicator).
    info_row: adw::ActionRow,
    mute_button: gtk::Button,
    default_button: gtk::Button,
    /// Individual port rows — always visible, one per port.
    port_rows: Vec<PortRow>,
}

/// A per-source row widget with interaction tracking state.
struct SourceRow {
    root: gtk::Box,
    source_name: String,
    slider: gtk::Scale,
    slider_handler_id: Rc<glib::SignalHandlerId>,
    interacting: Rc<RefCell<bool>>,
    pending_value: Rc<RefCell<Option<f64>>>,
    #[allow(dead_code)]
    pointer_down: Rc<RefCell<bool>>,
    #[allow(dead_code)]
    debounce_source: Rc<RefCell<Option<glib::SourceId>>>,
    info_row: adw::ActionRow,
    mute_button: gtk::Button,
    default_button: gtk::Button,
    /// Individual port rows — always visible, one per port.
    port_rows: Vec<PortRow>,
}

impl SinkRow {
    /// Set the slider volume, respecting active interaction state.
    ///
    /// During a drag/keyboard interaction the value is stashed and applied
    /// after the interaction ends so the slider is never yanked under the
    /// user's finger.
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

impl SourceRow {
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

/// A port row widget for an individual sink or source port.
///
/// Always visible, shows connected/disconnected status and a checkmark
/// when this port is the active port. Activatable only when connected.
struct PortRow {
    root: adw::ActionRow,
    port_name: String,
    /// Trailing icon indicating connected/disconnected state (dimmed when disconnected).
    status_icon: IconWidget,
    /// Trailing checkmark icon — visible only when this port is the active port.
    active_icon: IconWidget,
}

impl PortRow {
    /// Create a new port row.
    ///
    /// `on_activate` is called with the port's internal name when the row is
    /// activated (clicked). The closure produces the correct `AudioDeviceCardOutput`
    /// variant (SetSinkPort or SetSourcePort) for the parent device.
    fn new<F>(
        port: &AudioPort,
        active_port: Option<&str>,
        on_activate: F,
        output_cb: &OutputCallback,
    ) -> Self
    where
        F: Fn(String) -> AudioDeviceCardOutput + 'static,
    {
        let row = adw::ActionRow::builder()
            .title(&port.description)
            .build();

        let status_icon = IconWidget::from_name("audio-card-symbolic", 16);
        row.add_suffix(status_icon.widget());

        let active_icon = IconWidget::from_name("object-select-symbolic", 16);
        row.add_suffix(active_icon.widget());

        // Wire activation: only fires when the row is activatable (i.e. port is connected).
        {
            let cb = output_cb.clone();
            let port_name = port.name.clone();
            row.connect_activated(move |_| {
                if let Some(ref callback) = *cb.borrow() {
                    callback(on_activate(port_name.clone()));
                }
            });
        }

        let this = Self {
            root: row,
            port_name: port.name.clone(),
            status_icon,
            active_icon,
        };
        this.apply(port, active_port);
        this
    }

    /// Update the row to reflect the current port state.
    fn apply(&self, port: &AudioPort, active_port: Option<&str>) {
        let subtitle = if port.available {
            t("audio-port-connected")
        } else {
            t("audio-port-disconnected")
        };
        self.root.set_subtitle(&subtitle);

        if port.available {
            self.status_icon.widget().remove_css_class("dim-label");
            self.root.set_activatable(true);
            self.root.remove_css_class("dim-label");
        } else {
            self.status_icon.widget().add_css_class("dim-label");
            self.root.set_activatable(false);
            self.root.add_css_class("dim-label");
        }

        let is_active = active_port == Some(self.port_name.as_str());
        self.active_icon.widget().set_visible(is_active);
    }
}

/// A physical audio card widget for the settings page.
pub struct AudioDeviceCard {
    pub root: adw::PreferencesGroup,
    profile_row: adw::ComboRow,
    profile_model: gtk::StringList,
    sinks_box: gtk::Box,
    sources_box: gtk::Box,
    output_section_label: gtk::Label,
    input_section_label: gtk::Label,
    output_cb: OutputCallback,
    /// Suppresses change signals while applying props (used for profile/port combos).
    updating: Rc<RefCell<bool>>,
    /// Stored profile names for mapping combo index -> profile name.
    profile_names: Rc<RefCell<Vec<String>>>,
    /// Current sink sub-widgets keyed by sink_name.
    sink_rows: RefCell<Vec<SinkRow>>,
    /// Current source sub-widgets keyed by source_name.
    source_rows: RefCell<Vec<SourceRow>>,
}

impl AudioDeviceCard {
    pub fn new(card: &AudioCard) -> Self {
        let root = adw::PreferencesGroup::builder()
            .title(&card.name)
            .build();

        // Device-type icon in the group header (right side via header_suffix).
        {
            let header_icon = IconWidget::from_name(
                audio_device_icon(&card.device_type, AudioDeviceKind::Output),
                24,
            );
            root.set_header_suffix(Some(header_icon.widget()));
        }

        // Profile combo row
        let profile_model = gtk::StringList::new(&[]);
        let profile_row = adw::ComboRow::builder()
            .title(t("audio-card-profile"))
            .model(&profile_model)
            .build();
        root.add(&profile_row);

        // Section labels
        let output_section_label = gtk::Label::builder()
            .label(t("audio-output-devices"))
            .xalign(0.0)
            .css_classes(["heading"])
            .margin_top(12)
            .build();
        root.add(&output_section_label);

        let sinks_box = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .spacing(4)
            .build();
        root.add(&sinks_box);

        let input_section_label = gtk::Label::builder()
            .label(t("audio-input-devices"))
            .xalign(0.0)
            .css_classes(["heading"])
            .margin_top(12)
            .build();
        root.add(&input_section_label);

        let sources_box = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .spacing(4)
            .build();
        root.add(&sources_box);

        let output_cb: OutputCallback = Rc::new(RefCell::new(None));
        let updating = Rc::new(RefCell::new(false));
        let profile_names: Rc<RefCell<Vec<String>>> = Rc::new(RefCell::new(Vec::new()));

        // Wire profile combo change
        {
            let cb = output_cb.clone();
            let updating_ref = updating.clone();
            let names = profile_names.clone();
            profile_row.connect_selected_notify(move |combo| {
                if *updating_ref.borrow() {
                    return;
                }
                let idx = combo.selected() as usize;
                let names_ref = names.borrow();
                if let Some(profile_name) = names_ref.get(idx)
                    && let Some(ref callback) = *cb.borrow()
                {
                    callback(AudioDeviceCardOutput::SetProfile(
                        profile_name.clone(),
                    ));
                }
            });
        }

        let widget = Self {
            root,
            profile_row,
            profile_model,
            sinks_box,
            sources_box,
            output_section_label,
            input_section_label,
            output_cb,
            updating,
            profile_names,
            sink_rows: RefCell::new(Vec::new()),
            source_rows: RefCell::new(Vec::new()),
        };

        widget.apply_props(card);
        widget
    }

    /// Update the card widget to reflect new state.
    pub fn apply_props(&self, card: &AudioCard) {
        *self.updating.borrow_mut() = true;

        self.root.set_title(&card.name);

        // Update profiles
        {
            let available_profiles: Vec<&_> = card
                .profiles
                .iter()
                .filter(|p| p.available)
                .collect();

            self.profile_model.splice(
                0,
                self.profile_model.n_items(),
                &available_profiles
                    .iter()
                    .map(|p| p.description.as_str())
                    .collect::<Vec<_>>(),
            );

            let mut names = self.profile_names.borrow_mut();
            *names = available_profiles.iter().map(|p| p.name.clone()).collect();

            // Select active profile
            if let Some(idx) = names.iter().position(|n| n == &card.active_profile) {
                self.profile_row.set_selected(idx as u32);
            }

            // Hide profile row if only one available profile
            self.profile_row.set_visible(available_profiles.len() > 1);
        }

        // Update sinks
        self.reconcile_sinks(&card.sinks);
        self.output_section_label
            .set_visible(!card.sinks.is_empty() && !card.sources.is_empty());

        // Update sources
        self.reconcile_sources(&card.sources);
        self.input_section_label
            .set_visible(!card.sources.is_empty() && !card.sinks.is_empty());

        *self.updating.borrow_mut() = false;
    }

    /// Incrementally reconcile the sinks list.
    ///
    /// Existing rows are updated in place (volume, mute, default, ports), new
    /// rows are appended, and stale rows are removed from the box and dropped.
    fn reconcile_sinks(&self, sinks: &[AudioCardSink]) {
        let mut rows = self.sink_rows.borrow_mut();
        let current_names: Vec<&str> = sinks.iter().map(|s| s.sink_name.as_str()).collect();

        // Remove stale rows
        rows.retain(|row| {
            if current_names.contains(&row.sink_name.as_str()) {
                true
            } else {
                self.sinks_box.remove(&row.root);
                false
            }
        });

        // Update existing rows in place, create new rows for new sinks
        for sink in sinks {
            if let Some(existing) = rows.iter_mut().find(|r| r.sink_name == sink.sink_name) {
                // Volume — respects interaction guard
                existing.set_volume(sink.volume);

                // Mute button icon and tooltip
                let mute_icon = if sink.muted {
                    "audio-volume-muted-symbolic"
                } else {
                    "audio-volume-high-symbolic"
                };
                let mute_label = if sink.muted {
                    t_args("audio-card-unmute", &[("name", &sink.name)])
                } else {
                    t_args("audio-card-mute", &[("name", &sink.name)])
                };
                existing.mute_button.set_icon_name(mute_icon);
                existing.mute_button.set_tooltip_text(Some(&mute_label));
                existing
                    .mute_button
                    .update_property(&[gtk::accessible::Property::Label(&mute_label)]);

                // Default indicator
                if sink.default {
                    existing.info_row.set_subtitle(&t("audio-default-device"));
                    existing.default_button.set_visible(false);
                } else {
                    existing.info_row.set_subtitle("");
                    existing.default_button.set_visible(true);
                }

                // Port rows — update in place if count unchanged, rebuild otherwise.
                if existing.port_rows.len() == sink.ports.len() {
                    for (pr, port) in existing.port_rows.iter().zip(sink.ports.iter()) {
                        pr.apply(port, sink.active_port.as_deref());
                    }
                } else {
                    for pr in &existing.port_rows {
                        existing.root.remove(pr.root.upcast_ref::<gtk::Widget>());
                    }
                    existing.port_rows = sink.ports.iter().map(|port| {
                        let sink_name = existing.sink_name.clone();
                        let pr = PortRow::new(
                            port,
                            sink.active_port.as_deref(),
                            move |port_name| AudioDeviceCardOutput::SetSinkPort {
                                sink: sink_name.clone(),
                                port: port_name,
                            },
                            &self.output_cb,
                        );
                        existing.root.append(pr.root.upcast_ref::<gtk::Widget>());
                        pr
                    }).collect();
                }
            } else {
                let row = self.build_sink_row(sink);
                self.sinks_box.append(&row.root);
                rows.push(row);
            }
        }

        self.sinks_box.set_visible(!sinks.is_empty());
    }

    /// Incrementally reconcile the sources list.
    fn reconcile_sources(&self, sources: &[AudioCardSource]) {
        let mut rows = self.source_rows.borrow_mut();
        let current_names: Vec<&str> = sources.iter().map(|s| s.source_name.as_str()).collect();

        // Remove stale rows
        rows.retain(|row| {
            if current_names.contains(&row.source_name.as_str()) {
                true
            } else {
                self.sources_box.remove(&row.root);
                false
            }
        });

        // Update existing rows in place, create new rows for new sources
        for source in sources {
            if let Some(existing) = rows.iter_mut().find(|r| r.source_name == source.source_name) {
                existing.set_volume(source.volume);

                let mute_icon = if source.muted {
                    "audio-volume-muted-symbolic"
                } else {
                    "audio-volume-high-symbolic"
                };
                let mute_label = if source.muted {
                    t_args("audio-card-unmute", &[("name", &source.name)])
                } else {
                    t_args("audio-card-mute", &[("name", &source.name)])
                };
                existing.mute_button.set_icon_name(mute_icon);
                existing.mute_button.set_tooltip_text(Some(&mute_label));
                existing
                    .mute_button
                    .update_property(&[gtk::accessible::Property::Label(&mute_label)]);

                if source.default {
                    existing.info_row.set_subtitle(&t("audio-default-device"));
                    existing.default_button.set_visible(false);
                } else {
                    existing.info_row.set_subtitle("");
                    existing.default_button.set_visible(true);
                }

                // Port rows — update in place if count unchanged, rebuild otherwise.
                if existing.port_rows.len() == source.ports.len() {
                    for (pr, port) in existing.port_rows.iter().zip(source.ports.iter()) {
                        pr.apply(port, source.active_port.as_deref());
                    }
                } else {
                    for pr in &existing.port_rows {
                        existing.root.remove(pr.root.upcast_ref::<gtk::Widget>());
                    }
                    existing.port_rows = source.ports.iter().map(|port| {
                        let source_name = existing.source_name.clone();
                        let pr = PortRow::new(
                            port,
                            source.active_port.as_deref(),
                            move |port_name| AudioDeviceCardOutput::SetSourcePort {
                                source: source_name.clone(),
                                port: port_name,
                            },
                            &self.output_cb,
                        );
                        existing.root.append(pr.root.upcast_ref::<gtk::Widget>());
                        pr
                    }).collect();
                }
            } else {
                let row = self.build_source_row(source);
                self.sources_box.append(&row.root);
                rows.push(row);
            }
        }

        self.sources_box.set_visible(!sources.is_empty());
    }

    fn build_sink_row(&self, sink: &AudioCardSink) -> SinkRow {
        let root = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .spacing(0)
            .build();

        let icon = IconWidget::from_name(audio_device_icon(&sink.device_type, AudioDeviceKind::Output), 16);

        // Info row with icon, name, mute button, default button
        let info_row = adw::ActionRow::builder()
            .title(&sink.name)
            .activatable(false)
            .build();
        info_row.add_prefix(icon.widget());

        // Default button
        let default_button = gtk::Button::builder()
            .label(t("audio-set-default"))
            .valign(gtk::Align::Center)
            .css_classes(["flat"])
            .tooltip_text(t_args("audio-card-set-default-tooltip", &[("name", &sink.name)]))
            .build();
        info_row.add_suffix(&default_button);

        // Mute button
        let mute_icon = if sink.muted {
            "audio-volume-muted-symbolic"
        } else {
            "audio-volume-high-symbolic"
        };
        let mute_label = if sink.muted {
            t_args("audio-card-unmute", &[("name", &sink.name)])
        } else {
            t_args("audio-card-mute", &[("name", &sink.name)])
        };
        let mute_button = gtk::Button::builder()
            .icon_name(mute_icon)
            .valign(gtk::Align::Center)
            .css_classes(["flat"])
            .tooltip_text(&mute_label)
            .build();
        mute_button.update_property(&[gtk::accessible::Property::Label(&mute_label)]);
        info_row.add_suffix(&mute_button);

        root.append(&info_row);

        // Volume slider as separate row
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
        slider.set_value(sink.volume);
        slider.set_draw_value(false);

        // Wrap for gesture tracking: GestureClick on the same widget as
        // gtk::Scale gets cancelled by the scale's internal GestureDrag; a
        // parent wrapper box is not affected by that cancellation.
        let scale_wrapper = gtk::Box::new(gtk::Orientation::Horizontal, 0);
        scale_wrapper.set_hexpand(true);
        scale_wrapper.append(&slider);

        slider_row.append(&scale_wrapper);
        root.append(&slider_row);

        // Port rows — one adw::ActionRow per port, always visible.
        let port_rows: Vec<PortRow> = sink.ports.iter().map(|port| {
            let sink_name = sink.sink_name.clone();
            let pr = PortRow::new(
                port,
                sink.active_port.as_deref(),
                move |port_name| AudioDeviceCardOutput::SetSinkPort {
                    sink: sink_name.clone(),
                    port: port_name,
                },
                &self.output_cb,
            );
            root.append(pr.root.upcast_ref::<gtk::Widget>());
            pr
        }).collect();

        // Default indicator
        if sink.default {
            info_row.set_subtitle(&t("audio-default-device"));
            default_button.set_visible(false);
        } else {
            default_button.set_visible(true);
        }

        // Interaction tracking state
        let interacting: Rc<RefCell<bool>> = Rc::new(RefCell::new(false));
        let pointer_down: Rc<RefCell<bool>> = Rc::new(RefCell::new(false));
        let pending_value: Rc<RefCell<Option<f64>>> = Rc::new(RefCell::new(None));
        let debounce_source: Rc<RefCell<Option<glib::SourceId>>> = Rc::new(RefCell::new(None));

        // Wire value_changed — capture handler_id in Rc so set_volume() can
        // block/unblock without needing Clone on SignalHandlerId.
        let cb = self.output_cb.clone();
        let updating_ref = self.updating.clone();
        let name = sink.sink_name.clone();
        let interacting_vc = interacting.clone();
        let pointer_down_vc = pointer_down.clone();
        let debounce_source_vc = debounce_source.clone();
        let pending_vc = pending_value.clone();
        let slider_vc = slider.clone();

        // Temporary Rc to hold the handler_id; we populate it after connect.
        let handler_id_holder: Rc<RefCell<Option<glib::SignalHandlerId>>> =
            Rc::new(RefCell::new(None));
        let handler_id_holder_vc = handler_id_holder.clone();

        let raw_handler_id = slider.connect_value_changed(move |scale| {
            if *updating_ref.borrow() {
                return;
            }
            // Keyboard/scroll interaction path: pointer is not held down.
            if !*pointer_down_vc.borrow() {
                *interacting_vc.borrow_mut() = true;
                // Cancel any pending debounce and schedule a fresh one.
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
                        // Apply any backend value that arrived during interaction.
                        if let Some(v) = pending_d.borrow_mut().take() && let Some(ref hid) = *holder_d.borrow() {
                            slider_d.block_signal(hid);
                            slider_d.set_value(v);
                            slider_d.unblock_signal(hid);
                        }
                    },
                );
                *debounce_source_vc.borrow_mut() = Some(src);
            }
            if let Some(ref callback) = *cb.borrow() {
                callback(AudioDeviceCardOutput::SetSinkVolume {
                    sink: name.clone(),
                    volume: scale.value(),
                });
            }
        });

        // Store the handler_id so the debounce closure and set_volume can use it.
        *handler_id_holder.borrow_mut() = Some(raw_handler_id);

        // Wrap in Rc for sharing with gesture closures and schedule_interaction_end.
        // SAFETY: We only proceed if holder contains Some; the unwrap is
        // guaranteed because we just stored it above.
        let slider_handler_id: Rc<glib::SignalHandlerId> = Rc::new(
            handler_id_holder
                .borrow_mut()
                .take()
                .expect("handler_id must be stored"),
        );

        // GestureClick on the wrapper box for pointer press/release detection.
        let gesture = gtk::GestureClick::new();

        let interacting_pressed = interacting.clone();
        let pointer_down_pressed = pointer_down.clone();
        let debounce_pressed = debounce_source.clone();
        gesture.connect_pressed(move |_, _, _, _| {
            *pointer_down_pressed.borrow_mut() = true;
            *interacting_pressed.borrow_mut() = true;
            // Cancel any pending debounce — user is actively pressing.
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

        // Wire mute button
        let sink_name = sink.sink_name.clone();
        {
            let cb = self.output_cb.clone();
            let name = sink_name.clone();
            mute_button.connect_clicked(move |_| {
                if let Some(ref callback) = *cb.borrow() {
                    callback(AudioDeviceCardOutput::ToggleSinkMute {
                        sink: name.clone(),
                    });
                }
            });
        }

        // Wire default button
        {
            let cb = self.output_cb.clone();
            let name = sink_name.clone();
            default_button.connect_clicked(move |_| {
                if let Some(ref callback) = *cb.borrow() {
                    callback(AudioDeviceCardOutput::SetSinkDefault {
                        sink: name.clone(),
                    });
                }
            });
        }

        SinkRow {
            root,
            sink_name: sink.sink_name.clone(),
            slider,
            slider_handler_id,
            interacting,
            pending_value,
            pointer_down,
            debounce_source,
            info_row,
            mute_button,
            default_button,
            port_rows,
        }
    }

    fn build_source_row(&self, source: &AudioCardSource) -> SourceRow {
        let root = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .spacing(0)
            .build();

        let icon = IconWidget::from_name(audio_device_icon(&source.device_type, AudioDeviceKind::Input), 16);

        let info_row = adw::ActionRow::builder()
            .title(&source.name)
            .activatable(false)
            .build();
        info_row.add_prefix(icon.widget());

        // Default button
        let default_button = gtk::Button::builder()
            .label(t("audio-set-default"))
            .valign(gtk::Align::Center)
            .css_classes(["flat"])
            .tooltip_text(t_args("audio-card-set-default-tooltip", &[("name", &source.name)]))
            .build();
        info_row.add_suffix(&default_button);

        // Mute button
        let mute_icon = if source.muted {
            "audio-volume-muted-symbolic"
        } else {
            "audio-volume-high-symbolic"
        };
        let mute_label = if source.muted {
            t_args("audio-card-unmute", &[("name", &source.name)])
        } else {
            t_args("audio-card-mute", &[("name", &source.name)])
        };
        let mute_button = gtk::Button::builder()
            .icon_name(mute_icon)
            .valign(gtk::Align::Center)
            .css_classes(["flat"])
            .tooltip_text(&mute_label)
            .build();
        mute_button.update_property(&[gtk::accessible::Property::Label(&mute_label)]);
        info_row.add_suffix(&mute_button);

        root.append(&info_row);

        // Volume slider as separate row
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
        slider.set_value(source.volume);
        slider.set_draw_value(false);

        let scale_wrapper = gtk::Box::new(gtk::Orientation::Horizontal, 0);
        scale_wrapper.set_hexpand(true);
        scale_wrapper.append(&slider);

        slider_row.append(&scale_wrapper);
        root.append(&slider_row);

        // Port rows — one adw::ActionRow per port, always visible.
        let port_rows: Vec<PortRow> = source.ports.iter().map(|port| {
            let source_name = source.source_name.clone();
            let pr = PortRow::new(
                port,
                source.active_port.as_deref(),
                move |port_name| AudioDeviceCardOutput::SetSourcePort {
                    source: source_name.clone(),
                    port: port_name,
                },
                &self.output_cb,
            );
            root.append(pr.root.upcast_ref::<gtk::Widget>());
            pr
        }).collect();

        // Default indicator
        if source.default {
            info_row.set_subtitle(&t("audio-default-device"));
            default_button.set_visible(false);
        } else {
            default_button.set_visible(true);
        }

        // Interaction tracking state
        let interacting: Rc<RefCell<bool>> = Rc::new(RefCell::new(false));
        let pointer_down: Rc<RefCell<bool>> = Rc::new(RefCell::new(false));
        let pending_value: Rc<RefCell<Option<f64>>> = Rc::new(RefCell::new(None));
        let debounce_source: Rc<RefCell<Option<glib::SourceId>>> = Rc::new(RefCell::new(None));

        // Wire value_changed
        let cb = self.output_cb.clone();
        let updating_ref = self.updating.clone();
        let name = source.source_name.clone();
        let interacting_vc = interacting.clone();
        let pointer_down_vc = pointer_down.clone();
        let debounce_source_vc = debounce_source.clone();
        let pending_vc = pending_value.clone();
        let slider_vc = slider.clone();

        let handler_id_holder: Rc<RefCell<Option<glib::SignalHandlerId>>> =
            Rc::new(RefCell::new(None));
        let handler_id_holder_vc = handler_id_holder.clone();

        let raw_handler_id = slider.connect_value_changed(move |scale| {
            if *updating_ref.borrow() {
                return;
            }
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
                        if let Some(v) = pending_d.borrow_mut().take() && let Some(ref hid) = *holder_d.borrow() {
                            slider_d.block_signal(hid);
                            slider_d.set_value(v);
                            slider_d.unblock_signal(hid);
                        }
                    },
                );
                *debounce_source_vc.borrow_mut() = Some(src);
            }
            if let Some(ref callback) = *cb.borrow() {
                callback(AudioDeviceCardOutput::SetSourceVolume {
                    source: name.clone(),
                    volume: scale.value(),
                });
            }
        });

        *handler_id_holder.borrow_mut() = Some(raw_handler_id);

        let slider_handler_id: Rc<glib::SignalHandlerId> = Rc::new(
            handler_id_holder
                .borrow_mut()
                .take()
                .expect("handler_id must be stored"),
        );

        // GestureClick on wrapper
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

        // Wire mute button
        let source_name = source.source_name.clone();
        {
            let cb = self.output_cb.clone();
            let name = source_name.clone();
            mute_button.connect_clicked(move |_| {
                if let Some(ref callback) = *cb.borrow() {
                    callback(AudioDeviceCardOutput::ToggleSourceMute {
                        source: name.clone(),
                    });
                }
            });
        }

        // Wire default button
        {
            let cb = self.output_cb.clone();
            let name = source_name.clone();
            default_button.connect_clicked(move |_| {
                if let Some(ref callback) = *cb.borrow() {
                    callback(AudioDeviceCardOutput::SetSourceDefault {
                        source: name.clone(),
                    });
                }
            });
        }

        SourceRow {
            root,
            source_name: source.source_name.clone(),
            slider,
            slider_handler_id,
            interacting,
            pending_value,
            pointer_down,
            debounce_source,
            info_row,
            mute_button,
            default_button,
            port_rows,
        }
    }

    /// Register a callback for card output events.
    pub fn connect_output<F: Fn(AudioDeviceCardOutput) + 'static>(&self, callback: F) {
        *self.output_cb.borrow_mut() = Some(Box::new(callback));
    }
}
