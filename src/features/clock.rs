use crate::plugins::{FeatureToggle, Plugin, Slot, Widget};
use anyhow::Result;
use async_trait::async_trait;
use gtk::prelude::*;
use std::cell::{Cell, RefCell};

use std::time::Duration;

/// A clock widget plugin that renders a two-line date/time header (date + time).
///
/// Key behavior:
/// - **Lazy GTK construction**: GTK widgets are created the first time `widgets()` is called,
///   i.e. after GTK has been initialized (during/after `connect_activate` UI build).
/// - **Plugin owns the timer**: a `glib::timeout_add_local` updates labels periodically.
/// - **Widget-only plugin**: no feature toggles.
pub struct ClockPlugin {
    initialized: Cell<bool>,
    state: RefCell<Option<ClockState>>,
}

struct ClockState {
    root: gtk::Box,
    date_label: gtk::Label,
    time_label: gtk::Label,
    tick_source_id: Option<gtk::glib::SourceId>,
}

impl ClockPlugin {
    pub const PLUGIN_NAME: &'static str = "plugin::clock";

    pub fn new() -> Self {
        Self {
            initialized: Cell::new(false),
            state: RefCell::new(None),
        }
    }

    fn ensure_state(&self) {
        if self.state.borrow().is_some() {
            return;
        }

        // IMPORTANT: This must only be called after GTK is initialized.
        let root = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .spacing(2)
            .build();

        let date_label = gtk::Label::builder()
            .label("—")
            .xalign(0.0)
            .css_classes(["title-3", "dim-label"])
            .build();

        let time_label = gtk::Label::builder()
            .label("—")
            .xalign(0.0)
            .css_classes(["title-1"])
            .build();

        root.append(&date_label);
        root.append(&time_label);

        *self.state.borrow_mut() = Some(ClockState {
            root,
            date_label,
            time_label,
            tick_source_id: None,
        });
    }

    fn update_labels(date_label: &gtk::Label, time_label: &gtk::Label) {
        let now = match gtk::glib::DateTime::now_local() {
            Ok(dt) => dt,
            Err(_) => match gtk::glib::DateTime::now_utc() {
                Ok(dt) => dt,
                Err(_) => return,
            },
        };

        // `DateTime::format()` returns `Result<GString, BoolError>` in the gtk4/glib bindings.
        if let Ok(s) = now.format("%a, %d %b %Y") {
            date_label.set_label(s.as_str());
        }
        if let Ok(s) = now.format("%H:%M") {
            time_label.set_label(s.as_str());
        }
    }

    fn ensure_timer_running(&self) {
        // Timer requires widgets (labels) to exist.
        self.ensure_state();

        // If we're not initialized yet, do not start timers.
        if !self.initialized.get() {
            return;
        }

        let mut guard = self.state.borrow_mut();
        let Some(state) = guard.as_mut() else {
            return;
        };

        if state.tick_source_id.is_some() {
            return;
        }

        // Immediate update so the UI is correct on first display.
        Self::update_labels(&state.date_label, &state.time_label);

        // Use weak refs so the timer does not keep widgets alive on shutdown.
        let date_weak = state.date_label.downgrade();
        let time_weak = state.time_label.downgrade();

        let id = gtk::glib::timeout_add_local(Duration::from_secs(1), move || {
            let (Some(date_label), Some(time_label)) = (date_weak.upgrade(), time_weak.upgrade())
            else {
                // Widgets are gone; stop the source.
                return gtk::glib::ControlFlow::Break;
            };

            Self::update_labels(&date_label, &time_label);
            gtk::glib::ControlFlow::Continue
        });

        state.tick_source_id = Some(id);
    }

    fn stop_timer(&self) {
        let mut guard = self.state.borrow_mut();
        let Some(state) = guard.as_mut() else {
            return;
        };

        if let Some(id) = state.tick_source_id.take() {
            id.remove();
        }
    }
}

impl Default for ClockPlugin {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait(?Send)]
impl Plugin for ClockPlugin {
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn name(&self) -> &str {
        Self::PLUGIN_NAME
    }

    async fn initialize(&mut self) -> Result<()> {
        self.initialized.set(true);
        // Do NOT construct GTK widgets or start timers here: GTK may not be initialized yet.
        Ok(())
    }

    async fn cleanup(&mut self) -> Result<()> {
        self.stop_timer();
        Ok(())
    }

    fn feature_toggles(&self) -> Vec<FeatureToggle> {
        Vec::new()
    }

    fn widgets(&self) -> Vec<Widget> {
        // Construct widgets lazily (GTK must be initialized by now).
        self.ensure_state();

        // Start timer only once, and only after initialization.
        self.ensure_timer_running();

        let state = self.state.borrow();
        let root = state
            .as_ref()
            .expect("clock state must exist after ensure_state")
            .root
            .clone();

        vec![Widget {
            el: root,
            weight: 0,
            column: Slot::Top,
        }]
    }
}
