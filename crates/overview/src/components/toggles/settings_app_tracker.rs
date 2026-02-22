//! Settings app availability tracking.
//!
//! Both Bluetooth and Network toggles show a "Settings" button when the
//! `waft-settings` app entity is present. This module centralises discovery,
//! subscription with initial reconciliation, and settings button construction.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use gtk::glib;
use waft_protocol::{Urn, entity};
use waft_client::{EntityActionCallback, EntityStore};

use crate::ui::feature_toggles::menu_settings::{
    FeatureToggleMenuSettingsButton, FeatureToggleMenuSettingsButtonProps,
};

/// Tracks whether the waft-settings app entity is present.
///
/// Subscribes to app entity changes and calls `on_change(is_available)`
/// whenever the settings app appears or disappears. Performs initial
/// reconciliation via `idle_add_local_once` to catch entities already cached.
pub struct SettingsAppTracker {
    available: Rc<Cell<bool>>,
    urn: Rc<RefCell<Option<Urn>>>,
}

impl SettingsAppTracker {
    pub fn new(store: &Rc<EntityStore>, on_change: impl Fn(bool) + 'static) -> Self {
        let available = Rc::new(Cell::new(false));
        let urn: Rc<RefCell<Option<Urn>>> = Rc::new(RefCell::new(None));
        let on_change = Rc::new(on_change);

        let reconcile = {
            let available = available.clone();
            let urn = urn.clone();
            let store = store.clone();
            let on_change = on_change.clone();

            move || {
                let apps: Vec<(Urn, entity::app::App)> =
                    store.get_entities_typed(entity::app::ENTITY_TYPE);

                let settings_urn = find_settings_app_urn(&apps);
                let now_available = settings_urn.is_some();
                let was_available = available.get();

                *urn.borrow_mut() = settings_urn;
                available.set(now_available);

                if was_available != now_available {
                    on_change(now_available);
                }
            }
        };

        store.subscribe_type(entity::app::ENTITY_TYPE, reconcile.clone());
        glib::idle_add_local_once(reconcile);

        Self { available, urn }
    }

    /// Whether the waft-settings app is currently available.
    pub fn is_available(&self) -> bool {
        self.available.get()
    }

    /// Build a settings button that dispatches `open-page` to waft-settings.
    ///
    /// The button uses the tracker's stored URN at click time, so it remains
    /// correct if the URN changes between construction and click.
    pub fn build_settings_button(
        &self,
        action_callback: &EntityActionCallback,
        page: &'static str,
        label: String,
    ) -> FeatureToggleMenuSettingsButton {
        let button = FeatureToggleMenuSettingsButton::new(FeatureToggleMenuSettingsButtonProps {
            label,
        });

        let urn_ref = self.urn.clone();
        let cb = action_callback.clone();
        button.on_click(move |_| {
            if let Some(ref urn) = *urn_ref.borrow() {
                cb(
                    urn.clone(),
                    "open-page".to_string(),
                    serde_json::json!({ "page": page }),
                );
            }
        });

        button
    }
}

/// Find the waft-settings app entity URN.
///
/// Returns `Some(urn)` only for `internal-apps/app/waft-settings`.
pub fn find_settings_app_urn(apps: &[(Urn, entity::app::App)]) -> Option<Urn> {
    apps.iter()
        .find(|(urn, _)| urn.plugin() == "internal-apps" && urn.id() == "waft-settings")
        .map(|(urn, _)| urn.clone())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_app_entry(plugin: &str, id: &str) -> (Urn, entity::app::App) {
        let urn = Urn::new(plugin, entity::app::ENTITY_TYPE, id);
        let app = entity::app::App {
            name: "Test App".to_string(),
            icon: "test-icon".to_string(),
            available: true,
            keywords: vec![],
            description: None,
        };
        (urn, app)
    }

    #[test]
    fn settings_urn_found_when_internal_apps_present() {
        let apps = vec![make_app_entry("internal-apps", "waft-settings")];
        let expected = Urn::new("internal-apps", entity::app::ENTITY_TYPE, "waft-settings");
        assert_eq!(find_settings_app_urn(&apps), Some(expected));
    }

    #[test]
    fn settings_urn_none_when_only_xdg_apps_present() {
        let apps = vec![
            make_app_entry("xdg-apps", "firefox"),
            make_app_entry("xdg-apps", "nautilus"),
        ];
        assert_eq!(find_settings_app_urn(&apps), None);
    }

    #[test]
    fn settings_urn_found_among_mixed_app_entities() {
        let settings_urn = Urn::new("internal-apps", entity::app::ENTITY_TYPE, "waft-settings");
        let apps = vec![
            make_app_entry("xdg-apps", "firefox"),
            (
                settings_urn.clone(),
                entity::app::App {
                    name: "Settings".to_string(),
                    icon: "preferences-system-symbolic".to_string(),
                    available: true,
                    keywords: vec![],
                    description: None,
                },
            ),
        ];
        assert_eq!(find_settings_app_urn(&apps), Some(settings_urn));
    }

    #[test]
    fn settings_urn_none_when_no_apps() {
        assert_eq!(find_settings_app_urn(&[]), None);
    }
}
