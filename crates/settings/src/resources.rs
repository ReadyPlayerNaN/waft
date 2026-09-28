//! Compiled GTK resources used by the settings application.

use std::sync::Once;

use gtk::gio;
use gtk::glib;

static REGISTER: Once = Once::new();

/// Register the settings UI resource bundle with the process.
///
/// Registration is intentionally idempotent because application setup can be
/// exercised more than once by tests or embedding callers.
pub fn register() {
    REGISTER.call_once(|| {
        let bytes = include_bytes!(concat!(env!("OUT_DIR"), "/settings.gresource"));
        let resource = gio::Resource::from_data(&glib::Bytes::from_static(bytes))
            .expect("compiled settings resources must be valid");
        gio::resources_register(&resource);
    });
}
