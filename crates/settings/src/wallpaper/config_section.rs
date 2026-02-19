//! Wallpaper configuration section -- wallpaper directory and sync toggle.
//!
//! Dumb widget: receives data via `apply_props()`, emits events via `connect_output()`.

use std::cell::RefCell;
use std::rc::Rc;

use adw::prelude::*;

use crate::i18n::t;

/// Output events from the config section.
pub enum ConfigSectionOutput {
    /// Configuration changed.
    ConfigChanged {
        wallpaper_dir: String,
        sync: bool,
    },
}

type OutputCallback = Rc<RefCell<Option<Box<dyn Fn(ConfigSectionOutput)>>>>;

/// Wallpaper configuration controls widget.
pub struct ConfigSection {
    pub root: adw::PreferencesGroup,
    output_cb: OutputCallback,
    dir_row: adw::EntryRow,
    sync_row: adw::SwitchRow,
    updating: Rc<std::cell::Cell<bool>>,
}

impl ConfigSection {
    pub fn new() -> Self {
        let output_cb: OutputCallback = Rc::new(RefCell::new(None));
        let updating = Rc::new(std::cell::Cell::new(false));

        let group = adw::PreferencesGroup::builder()
            .title(t("wallpaper-config"))
            .build();

        // Wallpaper directory entry
        let dir_row = adw::EntryRow::builder()
            .title(t("wallpaper-dir"))
            .text("~/.config/waft/wallpapers")
            .build();
        group.add(&dir_row);

        // Sync toggle
        let sync_row = adw::SwitchRow::builder()
            .title(t("wallpaper-sync"))
            .subtitle(t("wallpaper-sync-sub"))
            .active(true)
            .build();
        group.add(&sync_row);

        // Wire change signals
        {
            let cb = output_cb.clone();
            let dir_ref = dir_row.clone();
            let sync_ref = sync_row.clone();
            let updating_ref = updating.clone();

            let emit = move || {
                if updating_ref.get() {
                    return;
                }
                if let Some(ref callback) = *cb.borrow() {
                    callback(ConfigSectionOutput::ConfigChanged {
                        wallpaper_dir: dir_ref.text().to_string(),
                        sync: sync_ref.is_active(),
                    });
                }
            };

            let emit_dir = emit.clone();
            dir_row.connect_changed(move |_| emit_dir());

            let emit_sync = emit;
            sync_row.connect_active_notify(move |_| emit_sync());
        }

        Self {
            root: group,
            output_cb,
            dir_row,
            sync_row,
            updating,
        }
    }

    /// Update the config controls with current state.
    pub fn apply_props(&self, wallpaper_dir: &str, sync: bool) {
        self.updating.set(true);
        self.dir_row.set_text(wallpaper_dir);
        self.sync_row.set_active(sync);
        self.updating.set(false);
    }

    /// Enable or disable controls.
    pub fn set_sensitive(&self, sensitive: bool) {
        self.dir_row.set_sensitive(sensitive);
        self.sync_row.set_sensitive(sensitive);
    }

    /// Register a callback for output events.
    pub fn connect_output<F: Fn(ConfigSectionOutput) + 'static>(&self, callback: F) {
        *self.output_cb.borrow_mut() = Some(Box::new(callback));
    }
}
