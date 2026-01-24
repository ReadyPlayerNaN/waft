//! Pure GTK4 Icon widget.
//!
//! Displays notification icons from themed names, file paths, or raw bytes.
//! Tries each icon hint in order until one succeeds, falling back to a default.

use gtk::prelude::*;

use crate::features::notifications::types::NotificationIcon;

const DEFAULT_ICON: &str = "dialog-information-symbolic";

/// Pure GTK4 icon widget - displays themed icons or textures.
pub struct IconWidget {
    image: gtk::Image,
}

impl IconWidget {
    /// Create a new icon widget, trying each icon hint until one succeeds.
    pub fn new(icon_hints: Vec<NotificationIcon>) -> Self {
        let image = gtk::Image::builder()
            .pixel_size(32)
            .valign(gtk::Align::Start)
            .build();

        Self::apply_first_valid_icon(&image, &icon_hints);

        Self { image }
    }

    /// Try each icon hint in order until one succeeds, falling back to default.
    fn apply_first_valid_icon(image: &gtk::Image, icon_hints: &[NotificationIcon]) {
        for hint in icon_hints {
            if Self::try_apply_icon(image, hint) {
                return;
            }
        }

        // All hints failed, use default
        // Note: set_paintable must be called BEFORE set_icon_name because
        // GTK4 Image displays based on the last property set
        image.set_paintable(gtk::gdk::Paintable::NONE);
        image.set_icon_name(Some(DEFAULT_ICON));
    }

    /// Try to apply an icon hint. Returns true if successful.
    fn try_apply_icon(image: &gtk::Image, icon: &NotificationIcon) -> bool {
        match icon {
            NotificationIcon::Themed(name) => {
                if let Some(resolved) = Self::try_resolve_themed_icon(name) {
                    image.set_paintable(gtk::gdk::Paintable::NONE);
                    image.set_icon_name(Some(&resolved));
                    return true;
                }
                false
            }
            NotificationIcon::FilePath(path) => {
                if let Ok(tex) = gtk::gdk::Texture::from_filename(path.as_ref()) {
                    image.set_icon_name(None);
                    image.set_paintable(Some(&tex));
                    return true;
                }
                false
            }
            NotificationIcon::Bytes(_b) => {
                // TODO: Implement icon parsing from bytes
                false
            }
        }
    }

    /// Try to resolve a themed icon name. Returns Some(name) if icon exists, None otherwise.
    fn try_resolve_themed_icon(name: &str) -> Option<String> {
        let display = gtk::gdk::Display::default()?;
        let icon_theme = gtk::IconTheme::for_display(&display);

        // Try the original name first
        if icon_theme.has_icon(name) {
            return Some(name.to_string());
        }

        // Try with -symbolic suffix
        let symbolic = format!("{}-symbolic", name);
        if icon_theme.has_icon(&symbolic) {
            return Some(symbolic);
        }

        // Try lowercase
        let lowercase = name.to_lowercase();
        if icon_theme.has_icon(&lowercase) {
            return Some(lowercase);
        }

        // Try lowercase with -symbolic
        let lowercase_symbolic = format!("{}-symbolic", lowercase);
        if icon_theme.has_icon(&lowercase_symbolic) {
            return Some(lowercase_symbolic);
        }

        None
    }

    /// Get a reference to the image widget.
    pub fn widget(&self) -> &gtk::Image {
        &self.image
    }
}
