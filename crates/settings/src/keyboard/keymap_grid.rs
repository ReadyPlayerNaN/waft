//! Keyboard grid visualization widget.
//!
//! Displays a visual representation of a keyboard layout using three
//! staggered rows of key caps matching the physical layout of a standard
//! keyboard's alpha block (top/home/bottom rows).

use adw::prelude::*;

use super::xkb_keymap::KeymapGrid;

/// Horizontal stagger offsets (in pixels) for each keyboard row.
///
/// Approximates the physical stagger of a standard ANSI keyboard:
/// - Number row (AE): no offset
/// - Top row (QWERTY): no offset
/// - Home row (ASDF): shifted right by ~1/4 key width (~20 px)
/// - Bottom row (ZXCV): shifted right by ~1/2 key width (~36 px)
const NUMBER_ROW_MARGIN: i32 = 0;
const TOP_ROW_MARGIN: i32 = 20;
const HOME_ROW_MARGIN: i32 = 40;
const BOTTOM_ROW_MARGIN: i32 = 56;

/// Keyboard grid visualization widget.
///
/// Shows four staggered rows of key caps (number/top/home/bottom) that visually
/// approximate the physical layout of a standard keyboard's alpha block.
#[derive(Clone)]
pub struct KeymapGridWidget {
    pub root: gtk::Box,
    number_row_box: gtk::Box,
    top_row_box: gtk::Box,
    home_row_box: gtk::Box,
    bottom_row_box: gtk::Box,
}

impl KeymapGridWidget {
    pub fn new() -> Self {
        let number_row_box = gtk::Box::builder()
            .orientation(gtk::Orientation::Horizontal)
            .spacing(4)
            .margin_start(NUMBER_ROW_MARGIN)
            .build();

        let top_row_box = gtk::Box::builder()
            .orientation(gtk::Orientation::Horizontal)
            .spacing(4)
            .margin_start(TOP_ROW_MARGIN)
            .build();

        let home_row_box = gtk::Box::builder()
            .orientation(gtk::Orientation::Horizontal)
            .spacing(4)
            .margin_start(HOME_ROW_MARGIN)
            .build();

        let bottom_row_box = gtk::Box::builder()
            .orientation(gtk::Orientation::Horizontal)
            .spacing(4)
            .margin_start(BOTTOM_ROW_MARGIN)
            .build();

        let root = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .spacing(4)
            .css_classes(["keymap-grid-container"])
            .halign(gtk::Align::Center)
            .build();

        root.append(&number_row_box);
        root.append(&top_row_box);
        root.append(&home_row_box);
        root.append(&bottom_row_box);

        Self {
            root,
            number_row_box,
            top_row_box,
            home_row_box,
            bottom_row_box,
        }
    }

    /// Populate the grid with key caps from a `KeymapGrid`.
    ///
    /// Clears any existing content and creates new key cap frames for each key.
    pub fn set_keymap(&self, keymap: &KeymapGrid) {
        // Remove all existing children from each row
        while let Some(child) = self.number_row_box.first_child() {
            self.number_row_box.remove(&child);
        }
        while let Some(child) = self.top_row_box.first_child() {
            self.top_row_box.remove(&child);
        }
        while let Some(child) = self.home_row_box.first_child() {
            self.home_row_box.remove(&child);
        }
        while let Some(child) = self.bottom_row_box.first_child() {
            self.bottom_row_box.remove(&child);
        }

        // Number row (AE keys)
        for label_text in &keymap.number_row {
            self.number_row_box.append(&create_key_cap(label_text));
        }

        // Top row (AD keys)
        for label_text in &keymap.top_row {
            self.top_row_box.append(&create_key_cap(label_text));
        }

        // Home row (AC keys)
        for label_text in &keymap.home_row {
            self.home_row_box.append(&create_key_cap(label_text));
        }

        // Bottom row (AB keys)
        for label_text in &keymap.bottom_row {
            self.bottom_row_box.append(&create_key_cap(label_text));
        }
    }

    /// Show or hide the widget.
    pub fn set_visible(&self, visible: bool) {
        self.root.set_visible(visible);
    }
}

/// Create a single key cap widget (frame containing a label).
fn create_key_cap(text: &str) -> gtk::Frame {
    let label = gtk::Label::builder()
        .label(text)
        .halign(gtk::Align::Center)
        .valign(gtk::Align::Center)
        .width_chars(2)
        .build();

    gtk::Frame::builder()
        .child(&label)
        .css_classes(["keyboard-key"])
        .build()
}
