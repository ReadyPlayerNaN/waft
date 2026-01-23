//! Pure GTK4 Icon widget.
//!
//! Displays notification icons from themed names, file paths, or raw bytes.

use std::sync::Arc;

use gtk::prelude::*;

use crate::features::notifications::types::NotificationIcon;

/// Pure GTK4 icon widget - displays themed icons or textures.
pub struct IconWidget {
    pub root: gtk::Box,
    image: gtk::Image,
}

impl IconWidget {
    /// Create a new icon widget with the given notification icon.
    pub fn new(icon: NotificationIcon) -> Self {
        let root = gtk::Box::new(gtk::Orientation::Horizontal, 0);

        let image = gtk::Image::builder()
            .pixel_size(32)
            .valign(gtk::Align::Start)
            .build();

        Self::apply_icon(&image, icon);

        root.append(&image);

        Self { root, image }
    }

    /// Update the icon being displayed.
    pub fn set_icon(&self, icon: NotificationIcon) {
        Self::apply_icon(&self.image, icon);
    }

    fn apply_icon(image: &gtk::Image, icon: NotificationIcon) {
        match icon {
            NotificationIcon::Themed(name) => {
                image.set_icon_name(Some(&name));
                image.set_paintable(gtk::gdk::Paintable::NONE);
            }
            NotificationIcon::FilePath(path) => {
                if let Ok(tex) = gtk::gdk::Texture::from_filename(path.as_ref()) {
                    image.set_paintable(Some(&tex));
                    image.set_icon_name(None);
                } else {
                    image.set_icon_name(Some("dialog-information-symbolic"));
                    image.set_paintable(gtk::gdk::Paintable::NONE);
                }
            }
            NotificationIcon::Bytes(_b) => {
                // TODO: Implement icon parsing from bytes
                image.set_icon_name(Some("dialog-information-symbolic"));
                image.set_paintable(gtk::gdk::Paintable::NONE);
            }
        }
    }

    /// Get a reference to the root widget.
    pub fn widget(&self) -> &gtk::Box {
        &self.root
    }
}
