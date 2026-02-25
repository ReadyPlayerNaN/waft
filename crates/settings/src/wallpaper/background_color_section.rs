//! Background colour settings section -- dumb widget.
//!
//! Displays a colour picker for the niri workspace background colour and a
//! "Clear" button to remove it. Reads/writes directly to niri config KDL.

use std::path::PathBuf;

use adw::prelude::*;

use crate::i18n::t;
use crate::kdl_niri_windows;

/// Background colour control for the Wallpaper page.
pub struct BackgroundColorSection {
    pub root: adw::PreferencesGroup,
}

impl BackgroundColorSection {
    pub fn new(config_path: &PathBuf) -> Self {
        let group = adw::PreferencesGroup::builder()
            .title(t("wallpaper-background-color"))
            .description(t("wallpaper-background-color-sub"))
            .build();

        // Colour picker row
        let dialog = gtk::ColorDialog::builder().with_alpha(false).build();
        let color_btn = gtk::ColorDialogButton::builder().dialog(&dialog).build();
        let color_row = adw::ActionRow::builder()
            .title(t("wallpaper-background-color"))
            .build();
        color_row.add_suffix(&color_btn);
        group.add(&color_row);

        // Clear button row
        let clear_btn = gtk::Button::builder()
            .label(t("wallpaper-background-color-clear"))
            .build();
        clear_btn.add_css_class("destructive-action");
        let clear_row = adw::ActionRow::builder().build();
        clear_row.add_suffix(&clear_btn);
        group.add(&clear_row);

        // Load initial state
        match kdl_niri_windows::load_background_color(config_path) {
            Ok(Some(hex)) => {
                if let Ok(rgba) = gtk::gdk::RGBA::parse(&hex) {
                    color_btn.set_rgba(&rgba);
                }
            }
            Ok(None) => {}
            Err(e) => {
                log::warn!("[wallpaper-bg] Failed to load background color: {e}");
            }
        }

        // Wire colour change
        {
            let path = config_path.clone();
            color_btn.connect_rgba_notify(move |btn| {
                let rgba = btn.rgba();
                let hex = format!(
                    "#{:02x}{:02x}{:02x}",
                    (rgba.red() * 255.0) as u8,
                    (rgba.green() * 255.0) as u8,
                    (rgba.blue() * 255.0) as u8,
                );
                if let Err(e) = kdl_niri_windows::save_background_color(&path, Some(&hex)) {
                    log::warn!("[wallpaper-bg] Failed to save background color: {e}");
                }
            });
        }

        // Wire clear button
        {
            let path = config_path.clone();
            clear_btn.connect_clicked(move |_| {
                if let Err(e) = kdl_niri_windows::save_background_color(&path, None) {
                    log::warn!("[wallpaper-bg] Failed to clear background color: {e}");
                }
            });
        }

        Self { root: group }
    }
}
