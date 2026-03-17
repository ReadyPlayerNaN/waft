//! Common page layout helpers for settings pages.

/// Build the standard root box used by all settings pages.
///
/// Vertical orientation, 24px spacing, 24/24/12/12 margins.
pub fn page_root() -> gtk::Box {
    gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .spacing(24)
        .margin_top(24)
        .margin_bottom(24)
        .margin_start(12)
        .margin_end(12)
        .build()
}
