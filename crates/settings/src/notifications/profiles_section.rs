//! Notification profiles section -- smart container.
//!
//! Subscribes to `notification-profile` and `notification-group` entity types.
//! Displays each profile as an expander row. Users add/remove groups from
//! profiles explicitly. Each added group shows per-rule dropdowns (hide,
//! suppress toast, suppress sound).

use std::cell::Cell;
use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::rc::Rc;

use adw::prelude::*;
use waft_client::{EntityActionCallback, EntityStore};
use crate::search_index::SearchIndex;
use waft_protocol::Urn;
use waft_protocol::entity::notification_filter::{
    GroupRule, NOTIFICATION_GROUP_ENTITY_TYPE, NOTIFICATION_PROFILE_ENTITY_TYPE,
    NotificationGroup, NotificationProfile, RuleValue,
};

use crate::i18n::t;
use crate::notifications::id_from_name;

/// Smart container for notification profiles display and editing.
pub struct ProfilesSection {
    pub root: adw::PreferencesGroup,
}

struct ProfileWidgets {
    expander: adw::ExpanderRow,
    /// Rows inside the expander (group rows + add-group row), tracked for teardown.
    content_rows: Vec<gtk::ListBoxRow>,
    updating: Rc<Cell<bool>>,
    current_profile: Rc<RefCell<NotificationProfile>>,
    urn: Urn,
}

struct CreateFormWidgets {
    wrapper: gtk::ListBoxRow,
}

fn rule_options() -> Vec<String> {
    vec![t("notif-rule-default"), t("notif-rule-on"), t("notif-rule-off")]
}

fn rule_value_to_index(value: RuleValue) -> u32 {
    match value {
        RuleValue::Default => 0,
        RuleValue::On => 1,
        RuleValue::Off => 2,
    }
}

fn index_to_rule_value(idx: u32) -> RuleValue {
    match idx {
        1 => RuleValue::On,
        2 => RuleValue::Off,
        _ => RuleValue::Default,
    }
}

fn send_profile_update(
    profile: &NotificationProfile,
    action_callback: &EntityActionCallback,
    urn: &Urn,
) {
    let params = match serde_json::to_value(profile) {
        Ok(v) => v,
        Err(e) => {
            log::warn!("[profiles-section] failed to serialize profile: {e}");
            return;
        }
    };
    action_callback(urn.clone(), "update-profile".to_string(), params);
}

impl ProfilesSection {
    /// Phase 1: Register static search entries without constructing widgets.
    pub fn register_search(idx: &mut SearchIndex) {
        let page_title = t("settings-notifications");
        idx.add_section_deferred("notifications", &page_title, &t("notif-profiles"), "notif-profiles");
    }

    pub fn new(
        entity_store: &Rc<EntityStore>,
        action_callback: &EntityActionCallback,
        search_index: &Rc<RefCell<SearchIndex>>,
    ) -> Self {
        let pref_group = adw::PreferencesGroup::builder()
            .title(t("notif-profiles"))
            .build();

        // Add header button
        let add_button = gtk::Button::builder()
            .icon_name("list-add-symbolic")
            .css_classes(["circular", "suggested-action"])
            .valign(gtk::Align::Center)
            .build();
        pref_group.set_header_suffix(Some(&add_button));

        // Empty state shown when no profiles exist
        let empty_state = adw::StatusPage::builder()
            .icon_name("view-paged-symbolic")
            .title(t("notif-no-profiles"))
            .description(t("notif-no-profiles-desc"))
            .build();
        pref_group.add(&empty_state);

        let widgets_map: Rc<RefCell<HashMap<String, ProfileWidgets>>> =
            Rc::new(RefCell::new(HashMap::new()));

        let create_form: Rc<RefCell<Option<CreateFormWidgets>>> = Rc::new(RefCell::new(None));

        // Backfill search entry widgets
        {
            let mut idx = search_index.borrow_mut();
            idx.backfill_widget("notifications", &t("notif-profiles"), None, Some(&pref_group));
        }

        // Wire "Add" button
        {
            let pref_ref = pref_group.clone();
            let cb = action_callback.clone();
            let form_ref = create_form.clone();
            add_button.connect_clicked(move |_| {
                Self::show_create_form(&form_ref, &pref_ref, &cb);
            });
        }

        let reconcile = {
            let store = entity_store.clone();
            let group_ref = pref_group.clone();
            let map_ref = widgets_map.clone();
            let cb = action_callback.clone();
            let empty_ref = empty_state;

            Rc::new(move || {
                let profiles: Vec<(Urn, NotificationProfile)> =
                    store.get_entities_typed(NOTIFICATION_PROFILE_ENTITY_TYPE);
                let groups: Vec<(Urn, NotificationGroup)> =
                    store.get_entities_typed(NOTIFICATION_GROUP_ENTITY_TYPE);
                Self::reconcile(&map_ref, &group_ref, &empty_ref, &profiles, &groups, &cb);
            })
        };

        // Subscribe to both entity types
        {
            let r = reconcile.clone();
            entity_store.subscribe_type(NOTIFICATION_PROFILE_ENTITY_TYPE, move || r());
        }
        {
            let r = reconcile.clone();
            entity_store.subscribe_type(NOTIFICATION_GROUP_ENTITY_TYPE, move || r());
        }

        // Initial reconciliation
        {
            let r = reconcile;
            gtk::glib::idle_add_local_once(move || r());
        }

        Self { root: pref_group }
    }

    fn reconcile(
        widgets_map: &Rc<RefCell<HashMap<String, ProfileWidgets>>>,
        pref_group: &adw::PreferencesGroup,
        empty_state: &adw::StatusPage,
        profiles: &[(Urn, NotificationProfile)],
        groups: &[(Urn, NotificationGroup)],
        action_callback: &EntityActionCallback,
    ) {
        let mut map = widgets_map.borrow_mut();
        let mut seen = HashSet::new();

        // Sort groups by order for consistent display
        let mut sorted_groups: Vec<_> = groups.iter().map(|(_, g)| g).collect();
        sorted_groups.sort_by_key(|g| g.order);

        // Sort profiles by name
        let mut sorted_profiles: Vec<_> = profiles.iter().collect();
        sorted_profiles.sort_by(|(_, a), (_, b)| a.name.cmp(&b.name));

        for (urn, profile) in &sorted_profiles {
            seen.insert(profile.id.clone());

            if let Some(existing) = map.get_mut(&profile.id) {
                existing.expander.set_title(&profile.name);
                *existing.current_profile.borrow_mut() = profile.clone();
                existing.urn = (*urn).clone();
                Self::rebuild_content(existing, &sorted_groups, action_callback);
            } else {
                let widgets =
                    Self::create_profile_widgets(urn, profile, &sorted_groups, action_callback);
                pref_group.add(&widgets.expander);
                map.insert(profile.id.clone(), widgets);
            }
        }

        // Remove stale profiles
        let to_remove: Vec<String> = map
            .keys()
            .filter(|k| !seen.contains(*k))
            .cloned()
            .collect();
        for key in to_remove {
            if let Some(widgets) = map.remove(&key) {
                pref_group.remove(&widgets.expander);
            }
        }

        // Toggle empty state visibility
        let has_profiles = !map.is_empty();
        empty_state.set_visible(!has_profiles);
    }

    fn create_profile_widgets(
        urn: &Urn,
        profile: &NotificationProfile,
        groups: &[&NotificationGroup],
        action_callback: &EntityActionCallback,
    ) -> ProfileWidgets {
        let expander = adw::ExpanderRow::builder()
            .title(&profile.name)
            .build();

        // Delete button suffix
        let delete_button = gtk::Button::builder()
            .icon_name("user-trash-symbolic")
            .css_classes(["flat", "destructive-action"])
            .valign(gtk::Align::Center)
            .build();
        {
            let profile_urn = urn.clone();
            let cb = action_callback.clone();
            delete_button.connect_clicked(move |_| {
                cb(
                    profile_urn.clone(),
                    "delete-profile".to_string(),
                    serde_json::Value::Null,
                );
            });
        }
        expander.add_suffix(&delete_button);

        let updating = Rc::new(Cell::new(false));
        let current_profile = Rc::new(RefCell::new(profile.clone()));

        let mut widgets = ProfileWidgets {
            expander,
            content_rows: Vec::new(),
            updating,
            current_profile,
            urn: urn.clone(),
        };

        Self::rebuild_content(&mut widgets, groups, action_callback);

        widgets
    }

    /// Tear down and rebuild all content rows inside a profile's expander.
    fn rebuild_content(
        widgets: &mut ProfileWidgets,
        groups: &[&NotificationGroup],
        action_callback: &EntityActionCallback,
    ) {
        // Remove old rows
        for row in widgets.content_rows.drain(..) {
            widgets.expander.remove(&row);
        }

        let profile = widgets.current_profile.borrow().clone();

        // Show groups that are in profile.rules and exist in the groups list
        for group in groups {
            let Some(rule) = profile.rules.get(&group.id) else {
                continue;
            };

            let row = Self::build_group_row(
                group,
                rule,
                &widgets.current_profile,
                &widgets.updating,
                action_callback,
                &widgets.urn,
            );
            widgets.expander.add_row(&row);
            widgets.content_rows.push(row);
        }

        // "Add Group" row with available (not yet added) groups
        let available: Vec<_> = groups
            .iter()
            .filter(|g| !profile.rules.contains_key(&g.id))
            .copied()
            .collect();
        if !available.is_empty() {
            let add_row = Self::build_add_group_row(
                &available,
                &widgets.current_profile,
                action_callback,
                &widgets.urn,
            );
            widgets.expander.add_row(&add_row);
            widgets.content_rows.push(add_row);
        }
    }

    fn build_group_row(
        group: &NotificationGroup,
        rule: &GroupRule,
        current_profile: &Rc<RefCell<NotificationProfile>>,
        updating: &Rc<Cell<bool>>,
        action_callback: &EntityActionCallback,
        urn: &Urn,
    ) -> gtk::ListBoxRow {
        let group_box = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .spacing(0)
            .build();

        let sub_group = adw::PreferencesGroup::builder()
            .title(&group.name)
            .margin_start(12)
            .margin_end(12)
            .margin_top(6)
            .margin_bottom(6)
            .build();

        // Remove button in header
        let remove_button = gtk::Button::builder()
            .icon_name("list-remove-symbolic")
            .css_classes(["flat"])
            .valign(gtk::Align::Center)
            .build();
        sub_group.set_header_suffix(Some(&remove_button));

        {
            let prof = current_profile.clone();
            let cb = action_callback.clone();
            let urn = urn.clone();
            let gid = group.id.clone();
            remove_button.connect_clicked(move |_| {
                let mut current = prof.borrow().clone();
                current.rules.remove(&gid);
                send_profile_update(&current, &cb, &urn);
                *prof.borrow_mut() = current;
            });
        }

        let hide_dropdown = Self::create_rule_dropdown();
        hide_dropdown.set_selected(rule_value_to_index(rule.hide));
        let hide_row = adw::ActionRow::builder().title(t("notif-rule-hide")).build();
        hide_row.add_suffix(&hide_dropdown);
        sub_group.add(&hide_row);

        let no_toast_dropdown = Self::create_rule_dropdown();
        no_toast_dropdown.set_selected(rule_value_to_index(rule.no_toast));
        let toast_row = adw::ActionRow::builder()
            .title(t("notif-rule-suppress-toast"))
            .build();
        toast_row.add_suffix(&no_toast_dropdown);
        sub_group.add(&toast_row);

        let no_sound_dropdown = Self::create_rule_dropdown();
        no_sound_dropdown.set_selected(rule_value_to_index(rule.no_sound));
        let suppress_sound_row = adw::ActionRow::builder()
            .title(t("notif-rule-suppress-sound"))
            .build();
        suppress_sound_row.add_suffix(&no_sound_dropdown);
        sub_group.add(&suppress_sound_row);

        let sound_entry = adw::EntryRow::builder()
            .title(t("notif-rule-custom-sound"))
            .text(rule.sound.as_deref().unwrap_or(""))
            .show_apply_button(true)
            .sensitive(rule.no_sound != RuleValue::On)
            .build();
        sub_group.add(&sound_entry);

        group_box.append(&sub_group);

        // Wire custom sound entry
        {
            let prof = current_profile.clone();
            let cb = action_callback.clone();
            let urn = urn.clone();
            let gid = group.id.clone();
            let guard = updating.clone();
            sound_entry.connect_apply(move |entry| {
                if guard.get() {
                    return;
                }
                let text = entry.text().to_string();
                let mut current = prof.borrow().clone();
                if let Some(rule) = current.rules.get_mut(&gid) {
                    rule.sound = if text.is_empty() { None } else { Some(text) };
                }
                send_profile_update(&current, &cb, &urn);
                *prof.borrow_mut() = current;
            });
        }

        // Wire rule dropdowns
        Self::wire_rule_dropdown(
            &hide_dropdown,
            "hide",
            updating,
            action_callback,
            urn,
            current_profile,
            &group.id,
        );
        Self::wire_rule_dropdown(
            &no_toast_dropdown,
            "no_toast",
            updating,
            action_callback,
            urn,
            current_profile,
            &group.id,
        );
        Self::wire_rule_dropdown_with_sound_entry(
            &no_sound_dropdown,
            "no_sound",
            updating,
            action_callback,
            urn,
            current_profile,
            &group.id,
            &sound_entry,
        );

        gtk::ListBoxRow::builder()
            .activatable(false)
            .selectable(false)
            .child(&group_box)
            .build()
    }

    fn wire_rule_dropdown(
        dropdown: &gtk::DropDown,
        field: &'static str,
        guard: &Rc<Cell<bool>>,
        action_callback: &EntityActionCallback,
        urn: &Urn,
        current_profile: &Rc<RefCell<NotificationProfile>>,
        group_id: &str,
    ) {
        let guard = guard.clone();
        let cb = action_callback.clone();
        let urn = urn.clone();
        let prof = current_profile.clone();
        let gid = group_id.to_string();

        dropdown.connect_selected_notify(move |dd| {
            if guard.get() {
                return;
            }
            guard.set(true);

            let new_value = index_to_rule_value(dd.selected());
            let mut current = prof.borrow().clone();

            let rule = current
                .rules
                .entry(gid.clone())
                .or_insert_with(|| GroupRule {
                    hide: RuleValue::Default,
                    no_toast: RuleValue::Default,
                    no_sound: RuleValue::Default,
                    sound: None,
                });

            match field {
                "hide" => rule.hide = new_value,
                "no_toast" => rule.no_toast = new_value,
                "no_sound" => rule.no_sound = new_value,
                _ => {}
            }

            send_profile_update(&current, &cb, &urn);
            *prof.borrow_mut() = current;
            guard.set(false);
        });
    }

    /// Like `wire_rule_dropdown`, but also updates the sound entry sensitivity
    /// when the no_sound dropdown changes.
    #[allow(clippy::too_many_arguments)]
    fn wire_rule_dropdown_with_sound_entry(
        dropdown: &gtk::DropDown,
        field: &'static str,
        guard: &Rc<Cell<bool>>,
        action_callback: &EntityActionCallback,
        urn: &Urn,
        current_profile: &Rc<RefCell<NotificationProfile>>,
        group_id: &str,
        sound_entry: &adw::EntryRow,
    ) {
        let guard = guard.clone();
        let cb = action_callback.clone();
        let urn = urn.clone();
        let prof = current_profile.clone();
        let gid = group_id.to_string();
        let entry_ref = sound_entry.clone();

        dropdown.connect_selected_notify(move |dd| {
            if guard.get() {
                return;
            }
            guard.set(true);

            let new_value = index_to_rule_value(dd.selected());
            let mut current = prof.borrow().clone();

            let rule = current
                .rules
                .entry(gid.clone())
                .or_insert_with(|| GroupRule {
                    hide: RuleValue::Default,
                    no_toast: RuleValue::Default,
                    no_sound: RuleValue::Default,
                    sound: None,
                });

            match field {
                "hide" => rule.hide = new_value,
                "no_toast" => rule.no_toast = new_value,
                "no_sound" => {
                    rule.no_sound = new_value;
                    entry_ref.set_sensitive(new_value != RuleValue::On);
                }
                _ => {}
            }

            send_profile_update(&current, &cb, &urn);
            *prof.borrow_mut() = current;
            guard.set(false);
        });
    }

    fn build_add_group_row(
        available_groups: &[&NotificationGroup],
        current_profile: &Rc<RefCell<NotificationProfile>>,
        action_callback: &EntityActionCallback,
        urn: &Urn,
    ) -> gtk::ListBoxRow {
        let row_box = gtk::Box::builder()
            .orientation(gtk::Orientation::Horizontal)
            .spacing(6)
            .margin_top(6)
            .margin_bottom(6)
            .margin_start(12)
            .margin_end(12)
            .build();

        let group_names: Vec<&str> = available_groups.iter().map(|g| g.name.as_str()).collect();
        let group_ids: Vec<String> = available_groups.iter().map(|g| g.id.clone()).collect();

        let model = gtk::StringList::new(&group_names);
        let dropdown = gtk::DropDown::builder()
            .model(&model)
            .selected(0)
            .hexpand(true)
            .valign(gtk::Align::Center)
            .build();

        let add_button = gtk::Button::builder()
            .label(t("notif-add-group"))
            .css_classes(["flat", "suggested-action"])
            .valign(gtk::Align::Center)
            .build();

        row_box.append(&dropdown);
        row_box.append(&add_button);

        {
            let prof = current_profile.clone();
            let cb = action_callback.clone();
            let urn = urn.clone();
            add_button.connect_clicked(move |_| {
                let idx = dropdown.selected() as usize;
                let group_id = match group_ids.get(idx) {
                    Some(id) => id.clone(),
                    None => return,
                };

                let mut current = prof.borrow().clone();
                current
                    .rules
                    .entry(group_id)
                    .or_insert_with(|| GroupRule {
                        hide: RuleValue::Default,
                        no_toast: RuleValue::Default,
                        no_sound: RuleValue::Default,
                        sound: None,
                    });

                send_profile_update(&current, &cb, &urn);
                *prof.borrow_mut() = current;
            });
        }

        gtk::ListBoxRow::builder()
            .activatable(false)
            .selectable(false)
            .child(&row_box)
            .build()
    }

    fn show_create_form(
        create_form: &Rc<RefCell<Option<CreateFormWidgets>>>,
        pref_group: &adw::PreferencesGroup,
        action_callback: &EntityActionCallback,
    ) {
        {
            let form = create_form.borrow();
            if form.is_some() {
                return;
            }
        }

        let form_box = gtk::Box::builder()
            .orientation(gtk::Orientation::Horizontal)
            .spacing(6)
            .margin_top(12)
            .margin_bottom(12)
            .margin_start(12)
            .margin_end(12)
            .build();

        let name_entry = adw::EntryRow::builder()
            .title(t("notif-profile-name"))
            .show_apply_button(false)
            .hexpand(true)
            .build();

        let entry_group = adw::PreferencesGroup::builder().build();
        entry_group.add(&name_entry);

        let create_button = gtk::Button::builder()
            .label(t("notif-create"))
            .css_classes(["pill", "suggested-action"])
            .valign(gtk::Align::Center)
            .build();

        let cancel_button = gtk::Button::builder()
            .label(t("notif-cancel"))
            .css_classes(["pill"])
            .valign(gtk::Align::Center)
            .build();

        form_box.append(&entry_group);
        form_box.append(&create_button);
        form_box.append(&cancel_button);

        let wrapper = gtk::ListBoxRow::builder()
            .activatable(false)
            .selectable(false)
            .child(&form_box)
            .build();
        pref_group.add(&wrapper);

        // Wire create
        {
            let name_ref = name_entry.clone();
            let cb = action_callback.clone();
            let form_ref = create_form.clone();
            let pref_ref = pref_group.clone();
            create_button.connect_clicked(move |_| {
                let name = name_ref.text().to_string();
                if name.trim().is_empty() {
                    return;
                }
                let id = id_from_name(&name);
                let profile = NotificationProfile {
                    id: id.clone(),
                    name,
                    rules: HashMap::new(),
                };
                let urn = Urn::new("notifications", "notification-profile", &id);
                let params = match serde_json::to_value(&profile) {
                    Ok(v) => v,
                    Err(e) => {
                        log::warn!("[profiles-section] failed to serialize profile: {e}");
                        return;
                    }
                };
                cb(urn, "create-profile".to_string(), params);
                Self::remove_create_form(&mut form_ref.borrow_mut(), &pref_ref);
            });
        }

        // Wire cancel
        {
            let form_ref = create_form.clone();
            let pref_ref = pref_group.clone();
            cancel_button.connect_clicked(move |_| {
                Self::remove_create_form(&mut form_ref.borrow_mut(), &pref_ref);
            });
        }

        *create_form.borrow_mut() = Some(CreateFormWidgets { wrapper });
    }

    fn remove_create_form(
        form: &mut Option<CreateFormWidgets>,
        pref_group: &adw::PreferencesGroup,
    ) {
        if let Some(widgets) = form.take() {
            pref_group.remove(&widgets.wrapper);
        }
    }

    fn create_rule_dropdown() -> gtk::DropDown {
        let options = rule_options();
        let refs: Vec<&str> = options.iter().map(std::string::String::as_str).collect();
        let string_list = gtk::StringList::new(&refs);
        gtk::DropDown::builder()
            .model(&string_list)
            .selected(0)
            .valign(gtk::Align::Center)
            .build()
    }
}
