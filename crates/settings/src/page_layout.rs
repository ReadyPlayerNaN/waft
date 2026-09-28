//! Common page layout helpers for settings pages.

/// Build the standard root box used by all settings pages.
///
/// The stable hierarchy and layout properties are defined in
/// `ui/page-root.ui`; pages remain responsible for appending their dynamic
/// sections in Rust.
pub fn page_root() -> gtk::Box {
    let builder = gtk::Builder::from_resource("/com/waft/settings/page-root.ui");
    builder
        .object("root")
        .expect("page-root.ui must contain a GtkBox named root")
}
