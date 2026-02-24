//! Wallpaper thumbnail widget -- displays a wallpaper image preview with filename.
//!
//! Dumb widget: receives data via constructor, emits no events (selection handled by FlowBox).

use gtk::prelude::*;

/// A single wallpaper thumbnail: scaled image + filename label.
pub struct ThumbnailWidget {
    pub root: gtk::Box,
    pub path: String,
}

const THUMBNAIL_WIDTH: i32 = 120;
const THUMBNAIL_HEIGHT: i32 = 80;

impl ThumbnailWidget {
    pub fn new(path: &str, filename: &str) -> Self {
        let root = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .spacing(4)
            .css_classes(["wallpaper-thumbnail"])
            .margin_top(4)
            .margin_bottom(4)
            .margin_start(4)
            .margin_end(4)
            .build();

        let image = gtk::Picture::builder()
            .width_request(THUMBNAIL_WIDTH)
            .height_request(THUMBNAIL_HEIGHT)
            .content_fit(gtk::ContentFit::Cover)
            .build();

        let file = gtk::gio::File::for_path(path);
        image.set_file(Some(&file));

        root.append(&image);

        let label = gtk::Label::builder()
            .label(filename)
            .ellipsize(gtk::pango::EllipsizeMode::End)
            .max_width_chars(14)
            .build();
        root.append(&label);

        Self {
            root,
            path: path.to_string(),
        }
    }

    /// Mark this thumbnail as selected or not.
    pub fn set_selected(&self, selected: bool) {
        if selected {
            self.root.add_css_class("selected");
        } else {
            self.root.remove_css_class("selected");
        }
    }
}
