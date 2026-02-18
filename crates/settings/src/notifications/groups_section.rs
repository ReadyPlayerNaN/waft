//! Notification groups section -- smart container.
//!
//! Subscribes to `notification-group` entity type. Displays each group
//! as an expander row with a read-only summary of its pattern matcher.
//! Supports inline create/edit/delete via `GroupForm`.

use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::rc::Rc;

use adw::prelude::*;
use waft_client::{EntityActionCallback, EntityStore};
use waft_protocol::Urn;
use waft_protocol::entity::notification_filter::{
    CombinatorOperator, MatchField, MatchOperator, NOTIFICATION_GROUP_ENTITY_TYPE,
    NotificationGroup, Pattern, RuleCombinator, RuleNode,
};

use crate::notifications::group_form::{GroupForm, GroupFormOutput};

const NEW_MARKER: &str = "__new__";

/// Smart container for notification groups display and editing.
pub struct GroupsSection {
    /// Outer container: holds the pref_group and the form side by side.
    pub root: gtk::Box,
}

struct GroupWidgets {
    expander: adw::ExpanderRow,
    rows: Vec<adw::ActionRow>,
}

struct GroupsSectionState {
    groups: HashMap<String, GroupWidgets>,
    editing: Option<String>,
    form: Option<GroupForm>,
}

impl GroupsSection {
    pub fn new(entity_store: &Rc<EntityStore>, action_callback: &EntityActionCallback) -> Self {
        let root = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .spacing(12)
            .build();

        let pref_group = adw::PreferencesGroup::builder()
            .title("Notification Groups")
            .build();

        // Add header button
        let add_button = gtk::Button::builder()
            .icon_name("list-add-symbolic")
            .css_classes(["circular", "suggested-action"])
            .valign(gtk::Align::Center)
            .build();
        pref_group.set_header_suffix(Some(&add_button));

        root.append(&pref_group);

        let state: Rc<RefCell<GroupsSectionState>> = Rc::new(RefCell::new(GroupsSectionState {
            groups: HashMap::new(),
            editing: None,
            form: None,
        }));

        // Wire "Add" button
        {
            let state_ref = state.clone();
            let root_ref = root.clone();
            let cb = action_callback.clone();
            add_button.connect_clicked(move |_| {
                Self::show_create_form(&state_ref, &root_ref, &cb);
            });
        }

        // Subscribe to notification-group entities
        {
            let store = entity_store.clone();
            let pref_ref = pref_group.clone();
            let state_ref = state.clone();
            let root_ref = root.clone();
            let cb = action_callback.clone();

            entity_store.subscribe_type(NOTIFICATION_GROUP_ENTITY_TYPE, move || {
                let entities: Vec<(Urn, NotificationGroup)> =
                    store.get_entities_typed(NOTIFICATION_GROUP_ENTITY_TYPE);
                Self::reconcile(&state_ref, &pref_ref, &root_ref, &entities, &cb);
            });
        }

        // Initial reconciliation
        {
            let store = entity_store.clone();
            let pref_ref = pref_group;
            let state_ref = state;
            let root_ref = root.clone();
            let cb = action_callback.clone();

            gtk::glib::idle_add_local_once(move || {
                let entities: Vec<(Urn, NotificationGroup)> =
                    store.get_entities_typed(NOTIFICATION_GROUP_ENTITY_TYPE);
                if !entities.is_empty() {
                    log::debug!(
                        "[groups-section] Initial reconciliation: {} groups",
                        entities.len()
                    );
                    Self::reconcile(&state_ref, &pref_ref, &root_ref, &entities, &cb);
                }
            });
        }

        Self { root }
    }

    fn reconcile(
        state: &Rc<RefCell<GroupsSectionState>>,
        pref_group: &adw::PreferencesGroup,
        root: &gtk::Box,
        entities: &[(Urn, NotificationGroup)],
        action_callback: &EntityActionCallback,
    ) {
        let mut st = state.borrow_mut();
        let mut seen = HashSet::new();

        // Sort by order
        let mut sorted: Vec<_> = entities.iter().collect();
        sorted.sort_by_key(|(_, g)| g.order);

        let editing_id = st.editing.clone();

        for (_, group) in &sorted {
            seen.insert(group.id.clone());

            // Skip reconciliation of the group currently being edited
            if editing_id.as_deref() == Some(&group.id) {
                continue;
            }

            if let Some(existing) = st.groups.get_mut(&group.id) {
                // Update existing expander title and subtitle
                existing.expander.set_title(&group.name);
                existing
                    .expander
                    .set_subtitle(&format!("Order: {}", group.order));

                // Remove old pattern rows and rebuild
                for row in existing.rows.drain(..) {
                    existing.expander.remove(&row);
                }
                let new_rows = build_matcher_rows(&group.matcher, 0);
                for row in &new_rows {
                    existing.expander.add_row(row);
                }
                existing.rows = new_rows;
            } else {
                let widgets =
                    Self::create_group_widgets(group, state, root, action_callback);
                pref_group.add(&widgets.expander);
                st.groups.insert(group.id.clone(), widgets);
            }
        }

        // Remove stale groups
        let to_remove: Vec<String> = st
            .groups
            .keys()
            .filter(|k| !seen.contains(*k))
            .cloned()
            .collect();
        for key in &to_remove {
            if let Some(widgets) = st.groups.remove(key) {
                pref_group.remove(&widgets.expander);
            }
        }

        // If the group being edited was deleted externally, remove the form
        if let Some(ref editing) = st.editing
            && editing != NEW_MARKER && to_remove.contains(editing)
        {
            Self::remove_form(&mut st, root);
        }
    }

    fn create_group_widgets(
        group: &NotificationGroup,
        state: &Rc<RefCell<GroupsSectionState>>,
        root: &gtk::Box,
        action_callback: &EntityActionCallback,
    ) -> GroupWidgets {
        let expander = adw::ExpanderRow::builder()
            .title(&group.name)
            .subtitle(format!("Order: {}", group.order))
            .build();

        // Edit button suffix
        let edit_button = gtk::Button::builder()
            .icon_name("document-edit-symbolic")
            .css_classes(["flat"])
            .valign(gtk::Align::Center)
            .build();
        expander.add_suffix(&edit_button);

        let rows = build_matcher_rows(&group.matcher, 0);
        for row in &rows {
            expander.add_row(row);
        }

        // Wire edit button
        {
            let group_data = group.clone();
            let state_ref = state.clone();
            let root_ref = root.clone();
            let cb = action_callback.clone();
            let expander_ref = expander.clone();
            edit_button.connect_clicked(move |_| {
                Self::show_edit_form(
                    &state_ref,
                    &root_ref,
                    &group_data,
                    &expander_ref,
                    &cb,
                );
            });
        }

        GroupWidgets { expander, rows }
    }

    fn show_create_form(
        state: &Rc<RefCell<GroupsSectionState>>,
        root: &gtk::Box,
        action_callback: &EntityActionCallback,
    ) {
        {
            let st = state.borrow();
            if st.editing.is_some() {
                return;
            }
        }

        let form = GroupForm::new(None);
        root.append(&form.root);

        // Wire form output
        {
            let state_ref = state.clone();
            let root_ref = root.clone();
            let cb = action_callback.clone();
            form.connect_output(move |output| match output {
                GroupFormOutput::SaveRequested => {
                    let group = {
                        let st = state_ref.borrow();
                        st.form.as_ref().and_then(|f| f.get_group())
                    };
                    if let Some(group) = group {
                        let urn =
                            Urn::new("notifications", "notification-group", &group.id);
                        let params = match serde_json::to_value(&group) {
                            Ok(v) => v,
                            Err(e) => {
                                log::warn!(
                                    "[groups-section] failed to serialize group: {e}"
                                );
                                return;
                            }
                        };
                        cb(urn, "create-group".to_string(), params);
                    }
                    Self::remove_form(&mut state_ref.borrow_mut(), &root_ref);
                }
                GroupFormOutput::Cancel => {
                    Self::remove_form(&mut state_ref.borrow_mut(), &root_ref);
                }
                GroupFormOutput::Delete(_) => {
                    // Create mode has no delete
                }
            });
        }

        let mut st = state.borrow_mut();
        st.editing = Some(NEW_MARKER.to_string());
        st.form = Some(form);
    }

    fn show_edit_form(
        state: &Rc<RefCell<GroupsSectionState>>,
        root: &gtk::Box,
        group: &NotificationGroup,
        expander: &adw::ExpanderRow,
        action_callback: &EntityActionCallback,
    ) {
        {
            let st = state.borrow();
            if st.editing.is_some() {
                return;
            }
        }

        // Hide the expander while editing
        expander.set_visible(false);

        let form = GroupForm::new(Some(group));
        root.append(&form.root);

        // Wire form output
        {
            let state_ref = state.clone();
            let root_ref = root.clone();
            let cb = action_callback.clone();
            let expander_ref = expander.clone();
            form.connect_output(move |output| match output {
                GroupFormOutput::SaveRequested => {
                    let group = {
                        let st = state_ref.borrow();
                        st.form.as_ref().and_then(|f| f.get_group())
                    };
                    if let Some(group) = group {
                        let urn =
                            Urn::new("notifications", "notification-group", &group.id);
                        let params = match serde_json::to_value(&group) {
                            Ok(v) => v,
                            Err(e) => {
                                log::warn!(
                                    "[groups-section] failed to serialize group: {e}"
                                );
                                return;
                            }
                        };
                        cb(urn, "update-group".to_string(), params);
                    }
                    expander_ref.set_visible(true);
                    Self::remove_form(&mut state_ref.borrow_mut(), &root_ref);
                }
                GroupFormOutput::Cancel => {
                    expander_ref.set_visible(true);
                    Self::remove_form(&mut state_ref.borrow_mut(), &root_ref);
                }
                GroupFormOutput::Delete(id) => {
                    let urn = Urn::new("notifications", "notification-group", &id);
                    cb(urn, "delete-group".to_string(), serde_json::Value::Null);
                    // Don't restore expander; reconciliation will remove it
                    Self::remove_form(&mut state_ref.borrow_mut(), &root_ref);
                }
            });
        }

        let mut st = state.borrow_mut();
        st.editing = Some(group.id.clone());
        st.form = Some(form);
    }

    fn remove_form(state: &mut GroupsSectionState, root: &gtk::Box) {
        if let Some(form) = state.form.take() {
            root.remove(&form.root);
        }
        state.editing = None;
    }
}

fn build_matcher_rows(combinator: &RuleCombinator, depth: usize) -> Vec<adw::ActionRow> {
    let mut rows = Vec::new();
    let indent = "  ".repeat(depth);
    let op_label = match combinator.operator {
        CombinatorOperator::And => "All match (AND)",
        CombinatorOperator::Or => "Any match (OR)",
    };

    let op_row = adw::ActionRow::builder()
        .title(format!("{indent}{op_label}"))
        .css_classes(["dim-label"])
        .build();
    rows.push(op_row);

    for child in &combinator.children {
        match child {
            RuleNode::Pattern(pattern) => {
                let child_indent = "  ".repeat(depth + 1);
                let desc = format_pattern(pattern);
                let row = adw::ActionRow::builder()
                    .title(format!("{child_indent}{desc}"))
                    .build();
                rows.push(row);
            }
            RuleNode::Combinator(sub) => {
                let sub_rows = build_matcher_rows(sub, depth + 1);
                rows.extend(sub_rows);
            }
        }
    }

    rows
}

fn format_pattern(pattern: &Pattern) -> String {
    let field = format_field(pattern.field);
    let op = format_operator(pattern.operator);
    format!("{field} {op} '{}'", pattern.value)
}

fn format_field(field: MatchField) -> &'static str {
    match field {
        MatchField::AppName => "App Name",
        MatchField::AppId => "App ID",
        MatchField::Title => "Title",
        MatchField::Body => "Body",
        MatchField::Category => "Category",
        MatchField::Urgency => "Urgency",
        MatchField::Workspace => "Workspace",
    }
}

fn format_operator(op: MatchOperator) -> &'static str {
    match op {
        MatchOperator::Equals => "equals",
        MatchOperator::NotEquals => "not equals",
        MatchOperator::Contains => "contains",
        MatchOperator::NotContains => "not contains",
        MatchOperator::StartsWith => "starts with",
        MatchOperator::NotStartsWith => "not starts with",
        MatchOperator::EndsWith => "ends with",
        MatchOperator::NotEndsWith => "not ends with",
        MatchOperator::MatchesRegex => "matches regex",
        MatchOperator::NotMatchesRegex => "not matches regex",
    }
}
