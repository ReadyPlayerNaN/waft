//! Wallpaper mode section -- mode selector, darkman availability banner, day segment display.
//!
//! Dumb widget: receives data via `apply_props()`, emits events via `connect_output()`.

use std::cell::RefCell;
use std::rc::Rc;

use adw::prelude::*;

use waft_protocol::entity::display::{DaySegment, WallpaperMode};

use crate::i18n::t;

/// Output events from the mode section.
pub enum ModeSectionOutput {
    /// User changed the wallpaper mode.
    ModeChanged { mode: String },
}

type OutputCallback = Rc<RefCell<Option<Box<dyn Fn(ModeSectionOutput)>>>>;

/// Mode options in combo row order.
const MODE_OPTIONS: &[&str] = &["static", "style-tracking", "day-tracking"];

/// Wallpaper mode selector widget.
pub struct ModeSection {
    pub root: adw::PreferencesGroup,
    output_cb: OutputCallback,
    mode_row: adw::ComboRow,
    darkman_banner: adw::Banner,
    segment_row: adw::ActionRow,
    updating: Rc<std::cell::Cell<bool>>,
}

impl ModeSection {
    pub fn new() -> Self {
        let output_cb: OutputCallback = Rc::new(RefCell::new(None));
        let updating = Rc::new(std::cell::Cell::new(false));

        let group = adw::PreferencesGroup::builder()
            .title(t("wallpaper-mode"))
            .build();

        // Darkman unavailable banner
        let darkman_banner = adw::Banner::builder()
            .title(t("wallpaper-mode-darkman-unavailable"))
            .revealed(false)
            .build();
        group.add(&darkman_banner);

        // Mode ComboRow
        let mode_model = gtk::StringList::new(&[
            &t("wallpaper-mode-static"),
            &t("wallpaper-mode-style-tracking"),
            &t("wallpaper-mode-day-tracking"),
        ]);
        let mode_row = adw::ComboRow::builder()
            .title(t("wallpaper-mode"))
            .model(&mode_model)
            .selected(0) // Static default
            .build();
        group.add(&mode_row);

        // Day segment display row (read-only, visible only in DayTracking mode)
        let segment_row = adw::ActionRow::builder()
            .title(t("wallpaper-current-segment"))
            .subtitle("")
            .visible(false)
            .build();
        group.add(&segment_row);

        // Wire mode change signal
        {
            let cb = output_cb.clone();
            let updating_ref = updating.clone();
            mode_row.connect_selected_notify(move |row| {
                if updating_ref.get() {
                    return;
                }
                let selected = row.selected() as usize;
                let mode = MODE_OPTIONS.get(selected).unwrap_or(&"static").to_string();
                if let Some(ref callback) = *cb.borrow() {
                    callback(ModeSectionOutput::ModeChanged { mode });
                }
            });
        }

        Self {
            root: group,
            output_cb,
            mode_row,
            darkman_banner,
            segment_row,
            updating,
        }
    }

    /// Update the mode section with current entity state.
    pub fn apply_props(
        &self,
        mode: &WallpaperMode,
        current_segment: Option<&DaySegment>,
        style_tracking_available: bool,
    ) {
        self.updating.set(true);

        // Set combo row to match mode
        let idx = match mode {
            WallpaperMode::Static => 0,
            WallpaperMode::StyleTracking => 1,
            WallpaperMode::DayTracking => 2,
        };
        self.mode_row.set_selected(idx);

        // Show darkman banner when style tracking is unavailable
        self.darkman_banner.set_revealed(!style_tracking_available);

        // Show segment row only in DayTracking mode
        let show_segment = matches!(mode, WallpaperMode::DayTracking);
        self.segment_row.set_visible(show_segment);

        if show_segment
            && let Some(segment) = current_segment
        {
            let label = segment_label(*segment);
            self.segment_row.set_subtitle(&label);
        }

        self.updating.set(false);
    }

    /// Register a callback for output events.
    pub fn connect_output<F: Fn(ModeSectionOutput) + 'static>(&self, callback: F) {
        *self.output_cb.borrow_mut() = Some(Box::new(callback));
    }
}

/// Human-readable label for a day segment (with time range).
fn segment_label(segment: DaySegment) -> String {
    match segment {
        DaySegment::EarlyMorning => t("wallpaper-segment-early-morning"),
        DaySegment::Morning => t("wallpaper-segment-morning"),
        DaySegment::Afternoon => t("wallpaper-segment-afternoon"),
        DaySegment::Evening => t("wallpaper-segment-evening"),
        DaySegment::Night => t("wallpaper-segment-night"),
        DaySegment::MidnightOil => t("wallpaper-segment-midnight-oil"),
    }
}
