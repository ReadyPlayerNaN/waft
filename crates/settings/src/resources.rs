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

#[cfg(test)]
mod tests {
    use super::*;
    use gtk::glib::prelude::StaticType;
    use gtk::prelude::*;

    #[test]
    fn compiled_bundle_contains_template_and_css_resources() {
        register();
        for path in [
            "/com/waft/settings/settings-shell.ui",
            "/com/waft/settings/wifi-network-row.ui",
            "/com/waft/settings/settings.css",
        ] {
            gio::resources_lookup_data(path, gio::ResourceLookupFlags::NONE)
                .unwrap_or_else(|error| panic!("resource {path} missing: {error}"));
        }
    }

    #[test]
    fn every_ui_template_can_be_loaded_by_gtk_builder() {
        register();
        if gtk::init().is_err() {
            eprintln!("skipping GTK template instantiation: no display is available");
            return;
        }
        let shell = gtk::Builder::from_resource("/com/waft/settings/settings-shell.ui");
        let sidebar_slot: gtk::Box = shell
            .object("sidebar_slot")
            .expect("settings shell must expose an expandable sidebar slot");
        let content_slot: gtk::Box = shell
            .object("content_slot")
            .expect("settings shell must expose an expandable content slot");
        assert!(sidebar_slot.vexpands());
        assert!(content_slot.vexpands());

        // Composite-template resources are registered and instantiated through
        // their Rust type; GtkBuilder cannot load a <template> declaration as
        // a standalone builder document.
        crate::wifi::network_row::NetworkRow::static_type();
        crate::wifi::network_row::NetworkRow::build(&crate::wifi::network_row::NetworkRowProps {
            ssid: "resource-test".into(),
            strength: 50,
            secure: false,
            connected: false,
            connecting: false,
            on_navigate: None,
        });
        for name in [
            "account-row.ui",
            "application-window.ui",
            "bind-row.ui",
            "bluetooth-adapter-group.ui",
            "connection-row.ui",
            "device-row.ui",
            "display-dark-mode-automation.ui",
            "display-night-light.ui",
            "display-toggle-navigation.ui",
            "entity-list-group.ui",
            "page-root.ui",
            "password-dialog.ui",
            "plugin-row.ui",
            "provider-picker-dialog.ui",
            "rename-dialog.ui",
            "search-result-row.ui",
            "search-results.ui",
            "section-combo.ui",
            "section-toggle.ui",
            "service-row.ui",
            "sidebar.ui",
            "settings-shell.ui",
            "settings-sub-page.ui",
            "startup-entry-dialog.ui",
            "startup-row.ui",
            "timer-row.ui",
            "variant-dialog.ui",
            "weather-preview-group.ui",
            "wallpaper-thumbnail.ui",
            "wifi-adapter-group.ui",
            "wifi-network-detail.ui",
            "wifi-share-dialog.ui",
            "wired-adapter-group.ui",
        ] {
            let path = format!("/com/waft/settings/{name}");
            gtk::Builder::from_resource(&path);
        }
    }
}
