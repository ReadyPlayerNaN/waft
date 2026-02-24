//! Wallpaper preview section -- shows current wallpaper path and set/random buttons.
//!
//! Dumb widget: receives data via `apply_props()`, emits events via `connect_output()`.

use std::cell::RefCell;
use std::rc::Rc;

use adw::prelude::*;

use crate::i18n::t;

/// Output events from the preview section.
pub enum PreviewSectionOutput {
    /// User selected a file via the file chooser.
    SetWallpaper(String),
    /// User clicked the Random button.
    Random,
}

type OutputCallback = Rc<RefCell<Option<Box<dyn Fn(PreviewSectionOutput)>>>>;

/// Wallpaper preview section widget.
pub struct PreviewSection {
    pub root: adw::PreferencesGroup,
    output_cb: OutputCallback,
    path_row: adw::ActionRow,
    browse_button: gtk::Button,
    random_button: gtk::Button,
    unavailable_banner: adw::Banner,
}

impl PreviewSection {
    pub fn new() -> Self {
        let output_cb: OutputCallback = Rc::new(RefCell::new(None));
        let group = adw::PreferencesGroup::builder()
            .title(t("wallpaper-current"))
            .build();

        // Unavailable banner
        let unavailable_banner = adw::Banner::builder()
            .title(t("wallpaper-unavailable"))
            .revealed(false)
            .build();
        group.add(&unavailable_banner);

        // Current wallpaper path display
        let path_row = adw::ActionRow::builder()
            .title(t("wallpaper-current-path"))
            .subtitle(t("wallpaper-none"))
            .build();
        group.add(&path_row);

        // Button box for Browse and Random
        let button_box = gtk::Box::builder()
            .orientation(gtk::Orientation::Horizontal)
            .spacing(8)
            .halign(gtk::Align::Center)
            .margin_top(8)
            .build();

        let browse_button = gtk::Button::builder()
            .label(t("wallpaper-browse"))
            .css_classes(["suggested-action"])
            .build();

        let random_button = gtk::Button::builder()
            .label(t("wallpaper-random"))
            .build();

        button_box.append(&browse_button);
        button_box.append(&random_button);
        group.add(&button_box);

        // Wire browse button -> file chooser
        {
            let cb = output_cb.clone();
            let btn = browse_button.clone();
            browse_button.connect_clicked(move |_| {
                let cb_inner = cb.clone();
                let dialog = gtk::FileDialog::builder()
                    .title(t("wallpaper-browse-title"))
                    .modal(true)
                    .build();

                // File filter for images
                let filter = gtk::FileFilter::new();
                filter.set_name(Some(&t("wallpaper-image-files")));
                filter.add_mime_type("image/png");
                filter.add_mime_type("image/jpeg");
                filter.add_mime_type("image/webp");
                filter.add_mime_type("image/gif");
                filter.add_mime_type("image/bmp");

                let filters = gtk::gio::ListStore::new::<gtk::FileFilter>();
                filters.append(&filter);
                dialog.set_filters(Some(&filters));
                dialog.set_default_filter(Some(&filter));

                // Get the window for modal parent
                let window = btn.root().and_then(|r| r.downcast::<gtk::Window>().ok());
                dialog.open(window.as_ref(), gtk::gio::Cancellable::NONE, move |result| {
                    if let Ok(file) = result
                        && let Some(path) = file.path()
                    {
                        let path_str = path.to_string_lossy().to_string();
                        if let Some(ref callback) = *cb_inner.borrow() {
                            callback(PreviewSectionOutput::SetWallpaper(path_str));
                        }
                    }
                });
            });
        }

        // Wire random button
        {
            let cb = output_cb.clone();
            random_button.connect_clicked(move |_| {
                if let Some(ref callback) = *cb.borrow() {
                    callback(PreviewSectionOutput::Random);
                }
            });
        }

        Self {
            root: group,
            output_cb,
            path_row,
            browse_button,
            random_button,
            unavailable_banner,
        }
    }

    /// Update the preview with current wallpaper state.
    pub fn apply_props(&self, current_wallpaper: Option<&str>, available: bool) {
        let subtitle = match current_wallpaper {
            Some(path) => {
                // Show just the filename for brevity, full path as tooltip
                let filename = std::path::Path::new(path)
                    .file_name()
                    .map(|f| f.to_string_lossy().to_string())
                    .unwrap_or_else(|| path.to_string());
                self.path_row.set_tooltip_text(Some(path));
                filename
            }
            None => {
                self.path_row.set_tooltip_text(None);
                t("wallpaper-none")
            }
        };

        self.path_row.set_subtitle(&subtitle);
        self.unavailable_banner.set_revealed(!available);
        self.browse_button.set_sensitive(available);
        self.random_button.set_sensitive(available);
    }

    /// Show or hide the browse button (only relevant in Static mode).
    pub fn set_browse_visible(&self, visible: bool) {
        self.browse_button.set_visible(visible);
    }

    /// Register a callback for output events.
    pub fn connect_output<F: Fn(PreviewSectionOutput) + 'static>(&self, callback: F) {
        *self.output_cb.borrow_mut() = Some(Box::new(callback));
    }
}
