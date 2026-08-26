use std::cell::Cell;
use std::rc::Rc;

use gtk::prelude::*;
use waft_protocol::entity;
use waft_ui_gtk::icons::IconWidget;
use waft_ui_gtk::menu_state::{menu_id_for_widget, toggle_menu};
use waft_ui_gtk::widgets::feature_toggle::{
    FeatureToggleOutput, FeatureToggleProps, FeatureToggleWidget,
};

use crate::i18n;
use crate::layout::types::WidgetFeatureToggle;
use crate::ui::feature_toggles::menu::FeatureToggleMenuWidget;
use waft_client::{EntityActionCallback, EntityStore};

pub struct PowerProfilesToggle {
    toggle: Rc<FeatureToggleWidget>,
    menu: FeatureToggleMenuWidget,
    available: Rc<Cell<bool>>,
}

impl PowerProfilesToggle {
    pub fn new(
        store: &Rc<EntityStore>,
        action_callback: &EntityActionCallback,
        menu_store: &Rc<waft_core::menu_state::MenuStore>,
        rebuild_callback: Rc<dyn Fn()>,
    ) -> Self {
        let menu_id = menu_id_for_widget("power-profiles-toggle");
        let menu = FeatureToggleMenuWidget::new();
        let toggle = Rc::new(FeatureToggleWidget::new(
            FeatureToggleProps {
                active: false,
                busy: false,
                details: None,
                expandable: true,
                icon: "power-profile-balanced-symbolic".to_string(),
                title: i18n::t("power-profiles-title"),
                menu_id: Some(menu_id.clone()),
                expanded: false,
            },
            Some(menu_store.clone()),
        ));
        let available = Rc::new(Cell::new(false));

        {
            let cb = action_callback.clone();
            let store_ref = store.clone();
            let menu_store = menu_store.clone();
            let menu_id = menu_id.clone();
            toggle.connect_output(move |output| match output {
                FeatureToggleOutput::Activate | FeatureToggleOutput::Deactivate => {
                    let entities: Vec<(waft_protocol::Urn, entity::power::PowerProfile)> =
                        store_ref.get_entities_typed(entity::power::POWER_PROFILE_ENTITY_TYPE);
                    if let Some((urn, profile)) = entities.first()
                        && let Some(target) =
                            toggle_target_profile(&profile.active_profile, &profile.profiles)
                    {
                        cb(
                            urn.clone(),
                            "set-profile".to_string(),
                            serde_json::json!({ "profile": target }),
                        );
                    }
                }
                FeatureToggleOutput::ExpandToggle(_) => toggle_menu(&menu_store, &menu_id),
            });
        }

        {
            let store_ref = store.clone();
            let toggle_ref = toggle.clone();
            let menu_ref = menu.clone();
            let available_ref = available.clone();
            let rebuild_callback = rebuild_callback.clone();
            let cb = action_callback.clone();

            store.subscribe_type(entity::power::POWER_PROFILE_ENTITY_TYPE, move || {
                let entities: Vec<(waft_protocol::Urn, entity::power::PowerProfile)> =
                    store_ref.get_entities_typed(entity::power::POWER_PROFILE_ENTITY_TYPE);
                reconcile_profiles(
                    &toggle_ref,
                    &menu_ref,
                    &available_ref,
                    &rebuild_callback,
                    &cb,
                    &entities,
                );
            });
        }

        {
            let store_ref = store.clone();
            let toggle_ref = toggle.clone();
            let menu_ref = menu.clone();
            let available_ref = available.clone();
            let rebuild_callback = rebuild_callback.clone();
            let cb = action_callback.clone();

            gtk::glib::idle_add_local_once(move || {
                let entities: Vec<(waft_protocol::Urn, entity::power::PowerProfile)> =
                    store_ref.get_entities_typed(entity::power::POWER_PROFILE_ENTITY_TYPE);
                reconcile_profiles(
                    &toggle_ref,
                    &menu_ref,
                    &available_ref,
                    &rebuild_callback,
                    &cb,
                    &entities,
                );
            });
        }

        Self {
            toggle,
            menu,
            available,
        }
    }

    pub fn as_feature_toggles(&self) -> Vec<Rc<WidgetFeatureToggle>> {
        if !self.available.get() {
            return Vec::new();
        }
        vec![Rc::new(WidgetFeatureToggle {
            id: "power-profiles-toggle".to_string(),
            weight: 325,
            toggle: (*self.toggle).clone(),
            menu: Some(self.menu.widget().clone()),
        })]
    }
}

fn reconcile_profiles(
    toggle: &FeatureToggleWidget,
    menu: &FeatureToggleMenuWidget,
    available: &Cell<bool>,
    rebuild_callback: &Rc<dyn Fn()>,
    action_callback: &EntityActionCallback,
    entities: &[(waft_protocol::Urn, entity::power::PowerProfile)],
) {
    let was_available = available.get();
    let now_available = !entities.is_empty();

    if let Some((urn, profile)) = entities.first() {
        toggle.set_active(profile.active_profile != "power-saver");
        toggle.set_details(Some(profile_label(&profile.active_profile)));
        toggle.set_expandable(!profile.profiles.is_empty());
        rebuild_menu(menu, urn, profile, action_callback);
    } else {
        toggle.set_active(false);
        toggle.set_details(None);
        clear_menu(menu);
    }

    if was_available != now_available {
        available.set(now_available);
        rebuild_callback();
    }
}

fn clear_menu(menu: &FeatureToggleMenuWidget) {
    while let Some(child) = menu.root.first_child() {
        menu.remove(&child);
    }
}

fn rebuild_menu(
    menu: &FeatureToggleMenuWidget,
    urn: &waft_protocol::Urn,
    profile: &entity::power::PowerProfile,
    action_callback: &EntityActionCallback,
) {
    clear_menu(menu);

    for (profile_name, current) in menu_rows(&profile.profiles, &profile.active_profile) {
        let row = gtk::Box::builder()
            .orientation(gtk::Orientation::Horizontal)
            .spacing(12)
            .css_classes(["menu-row", "clickable"])
            .build();

        let profile_label_text = profile_label(&profile_name);
        let label = gtk::Label::builder()
            .label(&profile_label_text)
            .hexpand(true)
            .xalign(0.0)
            .build();
        row.append(&label);

        if current {
            let icon = IconWidget::from_name("object-select-symbolic", 24);
            row.append(icon.widget());
        }

        let cb = action_callback.clone();
        let row_urn = urn.clone();
        let target_profile = profile_name.clone();
        let gesture = gtk::GestureClick::new();
        gesture.connect_released(move |_, _, _, _| {
            cb(
                row_urn.clone(),
                "set-profile".to_string(),
                serde_json::json!({ "profile": target_profile }),
            );
        });
        row.add_controller(gesture);
        menu.append(&row);
    }
}

fn profile_label(profile: &str) -> String {
    match profile {
        "power-saver" => i18n::t("power-profile-power-saver"),
        "balanced" => i18n::t("power-profile-balanced"),
        "performance" => i18n::t("power-profile-performance"),
        other => other.replace('-', " "),
    }
}

fn toggle_target_profile(current: &str, profiles: &[String]) -> Option<String> {
    if current == "power-saver" {
        profiles
            .iter()
            .find(|profile| profile.as_str() == "balanced")
            .or_else(|| {
                profiles
                    .iter()
                    .find(|profile| profile.as_str() != "power-saver")
            })
            .cloned()
    } else {
        profiles
            .iter()
            .find(|profile| profile.as_str() == "power-saver")
            .cloned()
    }
}

fn menu_rows(profiles: &[String], active_profile: &str) -> Vec<(String, bool)> {
    profiles
        .iter()
        .cloned()
        .map(|profile| {
            let current = profile == active_profile;
            (profile, current)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn toggle_target_profile_prefers_balanced_then_first_non_saver() {
        let profiles = vec![
            "power-saver".to_string(),
            "balanced".to_string(),
            "performance".to_string(),
        ];
        assert_eq!(
            toggle_target_profile("power-saver", &profiles),
            Some("balanced".to_string())
        );
        assert_eq!(
            toggle_target_profile("balanced", &profiles),
            Some("power-saver".to_string())
        );

        let fallback = vec!["power-saver".to_string(), "performance".to_string()];
        assert_eq!(
            toggle_target_profile("power-saver", &fallback),
            Some("performance".to_string())
        );
        assert_eq!(
            toggle_target_profile("performance", &fallback),
            Some("power-saver".to_string())
        );
    }

    #[test]
    fn toggle_target_profile_handles_missing_pairs() {
        assert_eq!(
            toggle_target_profile("balanced", &["balanced".to_string()]),
            None
        );
        assert_eq!(
            toggle_target_profile("power-saver", &["power-saver".to_string()]),
            None
        );
    }

    #[test]
    fn menu_rows_marks_only_current_profile() {
        let rows = menu_rows(
            &[
                "power-saver".to_string(),
                "balanced".to_string(),
                "performance".to_string(),
            ],
            "balanced",
        );
        assert_eq!(rows[0], ("power-saver".to_string(), false));
        assert_eq!(rows[1], ("balanced".to_string(), true));
        assert_eq!(rows[2], ("performance".to_string(), false));
    }
}
