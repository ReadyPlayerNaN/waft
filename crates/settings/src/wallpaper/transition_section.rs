//! Wallpaper transition section -- controls for transition type, fps, angle, duration.
//!
//! Dumb widget: receives data via `apply_props()`, emits events via `connect_output()`.

use std::cell::RefCell;
use std::rc::Rc;

use adw::prelude::*;

use crate::i18n::t;

/// Output events from the transition section.
pub enum TransitionSectionOutput {
    /// Transition configuration changed.
    TransitionChanged {
        transition_type: String,
        fps: u32,
        angle: u32,
        duration: f64,
    },
}

type OutputCallback = Rc<RefCell<Option<Box<dyn Fn(TransitionSectionOutput)>>>>;

/// Available swww transition types.
const TRANSITION_TYPES: &[&str] = &[
    "none", "simple", "fade", "wipe", "grow", "wave", "outer", "random",
];

/// Wallpaper transition controls widget.
pub struct TransitionSection {
    pub root: adw::PreferencesGroup,
    output_cb: OutputCallback,
    type_row: adw::ComboRow,
    fps_row: adw::SpinRow,
    angle_row: adw::SpinRow,
    duration_row: adw::SpinRow,
    updating: Rc<std::cell::Cell<bool>>,
}

impl TransitionSection {
    pub fn new() -> Self {
        let output_cb: OutputCallback = Rc::new(RefCell::new(None));
        let updating = Rc::new(std::cell::Cell::new(false));

        let group = adw::PreferencesGroup::builder()
            .title(t("wallpaper-transition"))
            .build();

        // Transition type ComboRow
        let type_model = gtk::StringList::new(TRANSITION_TYPES);
        let type_row = adw::ComboRow::builder()
            .title(t("wallpaper-transition-type"))
            .model(&type_model)
            .selected(2) // "fade" default
            .build();
        group.add(&type_row);

        // FPS SpinRow
        let fps_adj = gtk::Adjustment::new(60.0, 1.0, 240.0, 1.0, 10.0, 0.0);
        let fps_row = adw::SpinRow::builder()
            .title(t("wallpaper-transition-fps"))
            .adjustment(&fps_adj)
            .build();
        group.add(&fps_row);

        // Angle SpinRow
        let angle_adj = gtk::Adjustment::new(0.0, 0.0, 360.0, 1.0, 15.0, 0.0);
        let angle_row = adw::SpinRow::builder()
            .title(t("wallpaper-transition-angle"))
            .subtitle(t("wallpaper-transition-angle-sub"))
            .adjustment(&angle_adj)
            .build();
        group.add(&angle_row);

        // Duration SpinRow
        let duration_adj = gtk::Adjustment::new(1.0, 0.0, 30.0, 0.1, 1.0, 0.0);
        let duration_row = adw::SpinRow::builder()
            .title(t("wallpaper-transition-duration"))
            .subtitle(t("wallpaper-transition-duration-sub"))
            .adjustment(&duration_adj)
            .digits(1)
            .build();
        group.add(&duration_row);

        // Wire change signals
        {
            let cb = output_cb.clone();
            let type_row_ref = type_row.clone();
            let fps_row_ref = fps_row.clone();
            let angle_row_ref = angle_row.clone();
            let duration_row_ref = duration_row.clone();
            let updating_ref = updating.clone();

            let emit = move || {
                if updating_ref.get() {
                    return;
                }
                let selected = type_row_ref.selected() as usize;
                let transition_type = TRANSITION_TYPES
                    .get(selected)
                    .unwrap_or(&"fade")
                    .to_string();

                if let Some(ref callback) = *cb.borrow() {
                    callback(TransitionSectionOutput::TransitionChanged {
                        transition_type,
                        fps: fps_row_ref.value() as u32,
                        angle: angle_row_ref.value() as u32,
                        duration: duration_row_ref.value(),
                    });
                }
            };

            let emit_type = emit.clone();
            type_row.connect_selected_notify(move |_| emit_type());

            let emit_fps = emit.clone();
            fps_row.connect_value_notify(move |_| emit_fps());

            let emit_angle = emit.clone();
            angle_row.connect_value_notify(move |_| emit_angle());

            let emit_duration = emit;
            duration_row.connect_value_notify(move |_| emit_duration());
        }

        Self {
            root: group,
            output_cb,
            type_row,
            fps_row,
            angle_row,
            duration_row,
            updating,
        }
    }

    /// Update the transition controls with current state.
    pub fn apply_props(&self, transition_type: &str, fps: u32, angle: u32, duration: f64) {
        self.updating.set(true);

        // Set combo row to match transition type
        let idx = TRANSITION_TYPES
            .iter()
            .position(|t| *t == transition_type)
            .unwrap_or(2); // default to "fade"
        self.type_row.set_selected(idx as u32);

        self.fps_row.set_value(f64::from(fps));
        self.angle_row.set_value(f64::from(angle));
        self.duration_row.set_value(duration);

        self.updating.set(false);
    }

    /// Enable or disable controls.
    pub fn set_sensitive(&self, sensitive: bool) {
        self.type_row.set_sensitive(sensitive);
        self.fps_row.set_sensitive(sensitive);
        self.angle_row.set_sensitive(sensitive);
        self.duration_row.set_sensitive(sensitive);
    }

    /// Register a callback for output events.
    pub fn connect_output<F: Fn(TransitionSectionOutput) + 'static>(&self, callback: F) {
        *self.output_cb.borrow_mut() = Some(Box::new(callback));
    }
}
