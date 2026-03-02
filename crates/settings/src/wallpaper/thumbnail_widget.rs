//! Wallpaper thumbnail widget -- displays a wallpaper image preview with filename.
//!
//! Dumb widget: receives data via constructor, emits no events (selection handled by FlowBox).
//! Supports drag-and-drop (provides path as string) and double-click to open in default viewer.

use gtk::gdk;
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

        // DragSource: provides file path as string for inter-gallery moves
        let drag_source = gtk::DragSource::new();
        drag_source.set_actions(gdk::DragAction::MOVE);

        let path_for_drag = path.to_string();
        drag_source.connect_prepare(move |_source, _x, _y| {
            let value = gtk::glib::Value::from(&path_for_drag);
            Some(gdk::ContentProvider::for_value(&value))
        });

        let root_for_begin = root.clone();
        drag_source.connect_drag_begin(move |_source, _drag| {
            root_for_begin.add_css_class("dragging");
        });

        let root_for_end = root.clone();
        drag_source.connect_drag_end(move |_source, _drag, _delete_data| {
            root_for_end.remove_css_class("dragging");
        });

        root.add_controller(drag_source);

        // GestureClick: double-click opens image in default viewer
        let gesture = gtk::GestureClick::new();
        gesture.set_button(gdk::BUTTON_PRIMARY);

        let path_for_click = path.to_string();
        gesture.connect_pressed(move |_gesture, n_press, _x, _y| {
            if n_press == 2 {
                let uri = format!("file://{}", path_for_click);
                if let Err(e) =
                    gtk::gio::AppInfo::launch_default_for_uri(&uri, gtk::gio::AppLaunchContext::NONE)
                {
                    log::warn!("[wallpaper/thumbnail] failed to open image: {e}");
                }
            }
        });

        root.add_controller(gesture);

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
