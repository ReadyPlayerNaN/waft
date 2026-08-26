//! Power settings page -- smart container.
//!
//! Composes battery status and power profile controls from the `power` plugin.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use adw::prelude::*;
use waft_client::{EntityActionCallback, EntityStore};
use waft_protocol::Urn;
use waft_protocol::entity::power::{self, Battery, BatteryState, PowerProfile};
use waft_ui_gtk::icons::IconWidget;

use crate::i18n::t;
use crate::search_index::SearchIndex;

pub struct PowerPage {
    pub root: gtk::Box,
}

struct PowerPageState {
    battery_group: adw::PreferencesGroup,
    battery_icon: IconWidget,
    charge_row: adw::ActionRow,
    state_row: adw::ActionRow,
    time_row: adw::ActionRow,
    profile_group: adw::PreferencesGroup,
    profile_row: adw::ComboRow,
    profile_model: gtk::StringList,
    degraded_row: adw::ActionRow,
    empty_state: adw::StatusPage,
}

impl PowerPage {
    pub fn register_search(idx: &mut SearchIndex) {
        let page_title = t("settings-power");
        let battery_section = t("power-battery-section");
        let profile_section = t("power-profile-section");
        idx.add_section_deferred(
            "power",
            &page_title,
            &battery_section,
            "power-battery-section",
        );
        idx.add_section_deferred(
            "power",
            &page_title,
            &profile_section,
            "power-profile-section",
        );
        idx.add_input_deferred(
            "power",
            &page_title,
            &profile_section,
            &t("power-profile-row"),
            "power-profile-row",
        );
    }

    pub fn new(
        entity_store: &Rc<EntityStore>,
        action_callback: &EntityActionCallback,
        search_index: &Rc<RefCell<SearchIndex>>,
    ) -> Self {
        let root = crate::page_layout::page_root();

        let battery_group = adw::PreferencesGroup::builder()
            .title(t("power-battery-section"))
            .visible(false)
            .build();
        let charge_row = adw::ActionRow::builder().title(t("power-charge")).build();
        let battery_icon = IconWidget::from_name("battery-symbolic", 16);
        charge_row.add_prefix(battery_icon.widget());
        let state_row = adw::ActionRow::builder().title(t("power-state")).build();
        let time_row = adw::ActionRow::builder()
            .title(t("power-time"))
            .visible(false)
            .build();
        battery_group.add(&charge_row);
        battery_group.add(&state_row);
        battery_group.add(&time_row);
        root.append(&battery_group);

        let profile_model = gtk::StringList::new(&[]);
        let profile_group = adw::PreferencesGroup::builder()
            .title(t("power-profile-section"))
            .visible(false)
            .build();
        let profile_row = adw::ComboRow::builder()
            .title(t("power-profile-row"))
            .model(&profile_model)
            .build();
        let degraded_row = adw::ActionRow::builder()
            .title(t("power-performance-degraded"))
            .visible(false)
            .build();
        profile_group.add(&profile_row);
        profile_group.add(&degraded_row);
        root.append(&profile_group);

        let empty_state = adw::StatusPage::builder()
            .icon_name("power-profile-balanced-symbolic")
            .title(t("power-empty-title"))
            .description(t("power-empty-description"))
            .build();
        root.append(&empty_state);

        {
            let mut idx = search_index.borrow_mut();
            idx.backfill_widget(
                "power",
                &t("power-battery-section"),
                None,
                Some(&battery_group),
            );
            idx.backfill_widget(
                "power",
                &t("power-profile-section"),
                None,
                Some(&profile_group),
            );
            idx.backfill_widget(
                "power",
                &t("power-profile-section"),
                Some(&t("power-profile-row")),
                Some(&profile_row),
            );
        }

        let state = Rc::new(RefCell::new(PowerPageState {
            battery_group,
            battery_icon,
            charge_row,
            state_row,
            time_row,
            profile_group,
            profile_row: profile_row.clone(),
            profile_model,
            degraded_row,
            empty_state,
        }));
        let updating_profile = Rc::new(Cell::new(false));
        let profile_ids: Rc<RefCell<Vec<String>>> = Rc::new(RefCell::new(Vec::new()));
        let profile_urn: Rc<RefCell<Option<Urn>>> = Rc::new(RefCell::new(None));

        {
            let cb = action_callback.clone();
            let guard = updating_profile.clone();
            let ids = profile_ids.clone();
            let urn_ref = profile_urn.clone();
            profile_row.connect_selected_notify(move |row| {
                if guard.get() {
                    return;
                }
                let selected = row.selected() as usize;
                let Some(profile) = ids.borrow().get(selected).cloned() else {
                    return;
                };
                let Some(urn) = urn_ref.borrow().clone() else {
                    return;
                };
                cb(
                    urn,
                    "set-profile".to_string(),
                    serde_json::json!({ "profile": profile }),
                );
            });
        }

        crate::subscription::subscribe_dual_entities::<Battery, PowerProfile, _>(
            entity_store,
            power::ENTITY_TYPE,
            power::POWER_PROFILE_ENTITY_TYPE,
            {
                let state = state.clone();
                let updating_profile = updating_profile.clone();
                let profile_ids = profile_ids.clone();
                let profile_urn = profile_urn.clone();
                move |batteries, profiles| {
                    Self::reconcile(
                        &state,
                        &updating_profile,
                        &profile_ids,
                        &profile_urn,
                        &batteries,
                        &profiles,
                    )
                }
            },
        );

        Self { root }
    }

    fn reconcile(
        state: &Rc<RefCell<PowerPageState>>,
        updating_profile: &Rc<Cell<bool>>,
        profile_ids: &Rc<RefCell<Vec<String>>>,
        profile_urn: &Rc<RefCell<Option<Urn>>>,
        batteries: &[(Urn, Battery)],
        profiles: &[(Urn, PowerProfile)],
    ) {
        let state = state.borrow_mut();

        if let Some((_, battery)) = batteries.iter().find(|(_, battery)| battery.present) {
            state.battery_icon.set_icon(&battery.icon_name);
            state
                .charge_row
                .set_subtitle(&format!("{}%", battery.percentage.round() as i64));
            state
                .state_row
                .set_subtitle(&battery_state_label(battery.state));
            if let Some((time_title, time_text)) = battery_time_label(battery) {
                state.time_row.set_title(&time_title);
                state.time_row.set_subtitle(&time_text);
                state.time_row.set_visible(true);
            } else {
                state.time_row.set_visible(false);
            }
            state.battery_group.set_visible(true);
        } else {
            state.battery_group.set_visible(false);
            state.time_row.set_visible(false);
        }

        if let Some((urn, profile)) = profiles.first() {
            updating_profile.set(true);

            while state.profile_model.n_items() > 0 {
                state.profile_model.remove(0);
            }
            for label in profile
                .profiles
                .iter()
                .map(|profile| profile_label(profile))
            {
                state.profile_model.append(&label);
            }
            *profile_ids.borrow_mut() = profile.profiles.clone();
            *profile_urn.borrow_mut() = Some(urn.clone());

            let selected = profile
                .profiles
                .iter()
                .position(|candidate| candidate == &profile.active_profile)
                .unwrap_or(0);
            state.profile_row.set_selected(selected as u32);
            state.profile_group.set_visible(true);

            if let Some(reason) = profile.performance_degraded.as_deref() {
                state.degraded_row.set_subtitle(reason);
                state.degraded_row.set_visible(true);
            } else {
                state.degraded_row.set_visible(false);
            }

            updating_profile.set(false);
        } else {
            updating_profile.set(true);
            *profile_urn.borrow_mut() = None;
            profile_ids.borrow_mut().clear();
            state.profile_row.set_selected(gtk::INVALID_LIST_POSITION);
            while state.profile_model.n_items() > 0 {
                state.profile_model.remove(0);
            }
            state.profile_group.set_visible(false);
            state.degraded_row.set_visible(false);
            updating_profile.set(false);
        }

        state
            .empty_state
            .set_visible(!state.battery_group.is_visible() && !state.profile_group.is_visible());
    }
}

fn profile_label(profile: &str) -> String {
    match profile {
        "power-saver" => t("power-profile-power-saver"),
        "balanced" => t("power-profile-balanced"),
        "performance" => t("power-profile-performance"),
        other => other.replace('-', " "),
    }
}

fn battery_state_label(state: BatteryState) -> String {
    match state {
        BatteryState::Unknown => t("battery-unknown"),
        BatteryState::Charging => t("battery-charging"),
        BatteryState::Discharging => t("battery-discharging"),
        BatteryState::Empty => t("battery-empty"),
        BatteryState::FullyCharged => t("battery-fully-charged"),
        BatteryState::PendingCharge => t("battery-pending-charge"),
        BatteryState::PendingDischarge => t("battery-pending-discharge"),
    }
}

fn battery_time_label(battery: &Battery) -> Option<(String, String)> {
    let (title, seconds) = match battery.state {
        BatteryState::Charging if battery.time_to_full > 0 => {
            (t("power-time-to-full"), battery.time_to_full)
        }
        BatteryState::Discharging if battery.time_to_empty > 0 => {
            (t("power-time-remaining"), battery.time_to_empty)
        }
        _ => return None,
    };

    let minutes = seconds / 60;
    let time = if minutes < 1 {
        t("battery-time-less-than-minute")
    } else {
        let hours = minutes / 60;
        let remaining_minutes = minutes % 60;
        if hours > 0 {
            format!("{hours}h {remaining_minutes:02}m")
        } else {
            format!("{remaining_minutes}m")
        }
    };

    let subtitle = match battery.state {
        BatteryState::Charging => t_args("battery-time-to-full", &[("time", &time)]),
        BatteryState::Discharging => t_args("battery-time-remaining", &[("time", &time)]),
        _ => return None,
    };

    Some((title, subtitle))
}

fn t_args(key: &str, args: &[(&str, &str)]) -> String {
    crate::i18n::t_args(key, args)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn battery_time_label_formats_charge_and_discharge() {
        let charging = Battery {
            present: true,
            percentage: 50.0,
            state: BatteryState::Charging,
            icon_name: String::new(),
            time_to_empty: 0,
            time_to_full: 5400,
        };
        let charging_label = battery_time_label(&charging).expect("time label");
        assert_eq!(charging_label.0, t("power-time-to-full"));
        assert!(charging_label.1.contains("1h 30m"));

        let discharging = Battery {
            state: BatteryState::Discharging,
            time_to_empty: 1800,
            time_to_full: 0,
            ..charging
        };
        let discharging_label = battery_time_label(&discharging).expect("time label");
        assert_eq!(discharging_label.0, t("power-time-remaining"));
        assert!(discharging_label.1.contains("30m"));
    }

    #[test]
    fn profile_label_uses_backend_native_fallback() {
        assert_eq!(profile_label("custom-profile"), "custom profile");
    }
}
