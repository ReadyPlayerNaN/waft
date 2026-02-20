//! Per-card audio widget.
//!
//! Dumb widget displaying a physical audio card as an `adw::PreferencesGroup`
//! with profile selector, and per-sink/source rows with volume, mute, default,
//! and port controls.

use std::cell::RefCell;
use std::rc::Rc;

use adw::prelude::*;
use waft_protocol::entity::audio::{AudioCard, AudioCardSink, AudioCardSource, AudioPort};
use waft_ui_gtk::widgets::icon::IconWidget;

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
    /// Suppresses change signals while applying props.
    updating: Rc<RefCell<bool>>,
    /// Stored profile names for mapping combo index -> profile name.
    profile_names: Rc<RefCell<Vec<String>>>,
    /// Current sink sub-widgets keyed by sink_name.
    sink_rows: RefCell<Vec<SinkRow>>,
    /// Current source sub-widgets keyed by source_name.
    source_rows: RefCell<Vec<SourceRow>>,
}

struct SinkRow {
    root: gtk::Box,
}

struct SourceRow {
    root: gtk::Box,
}

impl AudioDeviceCard {
    pub fn new(card: &AudioCard) -> Self {
        let root = adw::PreferencesGroup::builder()
            .title(&card.name)
            .build();

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

    fn reconcile_sinks(&self, sinks: &[AudioCardSink]) {
        let mut rows = self.sink_rows.borrow_mut();

        // Remove all existing rows from the box
        for row in rows.iter() {
            self.sinks_box.remove(&row.root);
        }
        rows.clear();

        // Create new rows
        for sink in sinks {
            let row = self.build_sink_row(sink);
            self.sinks_box.append(&row.root);
            rows.push(row);
        }

        self.sinks_box.set_visible(!sinks.is_empty());
    }

    fn reconcile_sources(&self, sources: &[AudioCardSource]) {
        let mut rows = self.source_rows.borrow_mut();

        for row in rows.iter() {
            self.sources_box.remove(&row.root);
        }
        rows.clear();

        for source in sources {
            let row = self.build_source_row(source);
            self.sources_box.append(&row.root);
            rows.push(row);
        }

        self.sources_box.set_visible(!sources.is_empty());
    }

    fn build_sink_row(&self, sink: &AudioCardSink) -> SinkRow {
        let root = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .spacing(0)
            .build();

        let icon = IconWidget::from_name(&sink.icon, 16);

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

        slider_row.append(&slider);
        root.append(&slider_row);

        // Port combo row (if > 1 port)
        let port_model = gtk::StringList::new(&[]);
        let port_row = adw::ComboRow::builder()
            .title(t("audio-card-port"))
            .model(&port_model)
            .build();

        let port_names = self.populate_port_row(&port_row, &port_model, &sink.ports, sink.active_port.as_deref());
        port_row.set_visible(sink.ports.len() > 1);
        root.append(&port_row);

        // Default indicator
        if sink.default {
            info_row.set_subtitle(&t("audio-default-device"));
            default_button.set_visible(false);
        } else {
            default_button.set_visible(true);
        }

        // Wire callbacks
        let sink_name = sink.sink_name.clone();

        {
            let cb = self.output_cb.clone();
            let updating_ref = self.updating.clone();
            let name = sink_name.clone();
            slider.connect_value_changed(move |scale| {
                if *updating_ref.borrow() {
                    return;
                }
                if let Some(ref callback) = *cb.borrow() {
                    callback(AudioDeviceCardOutput::SetSinkVolume {
                        sink: name.clone(),
                        volume: scale.value(),
                    });
                }
            });
        }

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

        {
            let cb = self.output_cb.clone();
            let updating_ref = self.updating.clone();
            let name = sink_name.clone();
            let port_names_clone = port_names.clone();
            port_row.connect_selected_notify(move |combo| {
                if *updating_ref.borrow() {
                    return;
                }
                let idx = combo.selected() as usize;
                if let Some(port_name) = port_names_clone.get(idx)
                    && let Some(ref callback) = *cb.borrow()
                {
                    callback(AudioDeviceCardOutput::SetSinkPort {
                        sink: name.clone(),
                        port: port_name.clone(),
                    });
                }
            });
        }

        SinkRow { root }
    }

    fn build_source_row(&self, source: &AudioCardSource) -> SourceRow {
        let root = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .spacing(0)
            .build();

        let icon = IconWidget::from_name(&source.icon, 16);

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

        slider_row.append(&slider);
        root.append(&slider_row);

        // Port combo row
        let port_model = gtk::StringList::new(&[]);
        let port_row = adw::ComboRow::builder()
            .title(t("audio-card-port"))
            .model(&port_model)
            .build();

        let port_names = self.populate_port_row(&port_row, &port_model, &source.ports, source.active_port.as_deref());
        port_row.set_visible(source.ports.len() > 1);
        root.append(&port_row);

        // Default indicator
        if source.default {
            info_row.set_subtitle(&t("audio-default-device"));
            default_button.set_visible(false);
        } else {
            default_button.set_visible(true);
        }

        // Wire callbacks
        let source_name = source.source_name.clone();

        {
            let cb = self.output_cb.clone();
            let updating_ref = self.updating.clone();
            let name = source_name.clone();
            slider.connect_value_changed(move |scale| {
                if *updating_ref.borrow() {
                    return;
                }
                if let Some(ref callback) = *cb.borrow() {
                    callback(AudioDeviceCardOutput::SetSourceVolume {
                        source: name.clone(),
                        volume: scale.value(),
                    });
                }
            });
        }

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

        {
            let cb = self.output_cb.clone();
            let updating_ref = self.updating.clone();
            let name = source_name.clone();
            let port_names_clone = port_names.clone();
            port_row.connect_selected_notify(move |combo| {
                if *updating_ref.borrow() {
                    return;
                }
                let idx = combo.selected() as usize;
                if let Some(port_name) = port_names_clone.get(idx)
                    && let Some(ref callback) = *cb.borrow()
                {
                    callback(AudioDeviceCardOutput::SetSourcePort {
                        source: name.clone(),
                        port: port_name.clone(),
                    });
                }
            });
        }

        SourceRow { root }
    }

    fn populate_port_row(
        &self,
        row: &adw::ComboRow,
        model: &gtk::StringList,
        ports: &[AudioPort],
        active_port: Option<&str>,
    ) -> Vec<String> {
        let descriptions: Vec<&str> = ports.iter().map(|p| p.description.as_str()).collect();
        model.splice(0, model.n_items(), &descriptions);

        let names: Vec<String> = ports.iter().map(|p| p.name.clone()).collect();

        if let Some(active) = active_port
            && let Some(idx) = names.iter().position(|n| n == active)
        {
            row.set_selected(idx as u32);
        }

        names
    }

    /// Register a callback for card output events.
    pub fn connect_output<F: Fn(AudioDeviceCardOutput) + 'static>(&self, callback: F) {
        *self.output_cb.borrow_mut() = Some(Box::new(callback));
    }
}
