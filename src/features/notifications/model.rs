use std::{collections::HashMap, time::SystemTime};

use super::types::{Notification, NotificationGroup, NotificationsSnapshot};

/// Normalized key used for grouping and (optionally) for app-name icon lookup.
fn normalize_app_key(app_name: &str) -> String {
    let mut out = String::with_capacity(app_name.len());
    let mut prev_dash = false;

    for ch in app_name.chars() {
        let c = ch.to_ascii_lowercase();

        // Keep common desktop/app-id characters.
        let is_ok = c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '.';
        let mapped = if is_ok {
            Some(c)
        } else if c.is_whitespace() || c == '/' || c == ':' {
            Some('-')
        } else {
            // Drop other punctuation/symbols.
            None
        };

        if let Some(mc) = mapped {
            if mc == '-' {
                if prev_dash {
                    continue;
                }
                prev_dash = true;
                out.push('-');
            } else {
                prev_dash = false;
                out.push(mc);
            }
        }
    }

    // Trim leading/trailing dashes.
    while out.starts_with('-') {
        out.remove(0);
    }
    while out.ends_with('-') {
        out.pop();
    }

    out
}

fn systemtime_cmp_desc(a: &SystemTime, b: &SystemTime) -> std::cmp::Ordering {
    // SystemTime doesn't implement Ord; use duration since UNIX_EPOCH if possible.
    // If times are before UNIX_EPOCH or errors occur, fall back to equality-ish ordering.
    use std::time::UNIX_EPOCH;

    match (a.duration_since(UNIX_EPOCH), b.duration_since(UNIX_EPOCH)) {
        (Ok(da), Ok(db)) => db.cmp(&da),
        _ => std::cmp::Ordering::Equal,
    }
}

/// Testable notifications model.
///
/// Stores notifications grouped by normalized app name key.
/// Generates sorted snapshots for rendering.
///
/// Grouping rules:
/// - Group strictly by a normalized app name key.
/// - Groups are ordered by most recent notification timestamp (descending).
/// - Within a group, notifications are ordered by most recent timestamp (descending).
/// - Only one group can be expanded at a time (`open_group`).
#[derive(Debug, Default)]
pub struct NotificationsModel {
    groups: HashMap<String, NotificationGroup>,
    open_group: Option<String>,
}

impl NotificationsModel {
    pub fn new() -> Self {
        Self::default()
    }

    /// Add a notification (inserts into its group) and ensures ordering.
    pub fn add(&mut self, n: Notification) {
        let key = normalize_app_key(&n.app_name);
        let group = self
            .groups
            .entry(key.clone())
            .or_insert_with(|| NotificationGroup {
                app_key: key.clone(),
                display_app_name: n.app_name.clone(),
                notifications: vec![],
            });

        // Replace existing with same id, if present (policy: last write wins).
        if let Some(pos) = group.notifications.iter().position(|x| x.id == n.id) {
            group.notifications.remove(pos);
        }

        group.notifications.push(n);

        // Sort newest-first, stable-ish by created_at then id.
        group.notifications.sort_by(|a, b| {
            let c = systemtime_cmp_desc(&a.created_at, &b.created_at);
            if c == std::cmp::Ordering::Equal {
                b.id.cmp(&a.id)
            } else {
                c
            }
        });
    }

    /// Fetch a notification by id (cloned), searching all groups.
    ///
    /// This is used by the toast popup to resolve toast ids into full notification payloads
    /// while keeping the overlay history as the single source of truth.
    pub fn get_by_id(&self, id: u64) -> Option<Notification> {
        for g in self.groups.values() {
            if let Some(n) = g.notifications.iter().find(|n| n.id == id) {
                return Some(n.clone());
            }
        }
        None
    }

    /// Remove a notification by id. Returns true if removed.
    pub fn remove(&mut self, id: u64) -> bool {
        let mut removed = false;
        let mut empty_keys: Vec<String> = vec![];

        for (k, g) in self.groups.iter_mut() {
            let before = g.notifications.len();
            g.notifications.retain(|n| n.id != id);
            if g.notifications.len() != before {
                removed = true;
            }
            if g.notifications.is_empty() {
                empty_keys.push(k.clone());
            }
        }

        for k in empty_keys {
            self.groups.remove(&k);
            if self.open_group.as_deref() == Some(&k) {
                self.open_group = None;
            }
        }

        removed
    }

    /// Clear all notifications and close any open group.
    pub fn clear(&mut self) {
        self.groups.clear();
        self.open_group = None;
    }

    /// Set which group is open. Only one may be open at a time.
    ///
    /// If `app_key` is `None`, closes all.
    /// If a key is provided but no such group exists (any more), open is cleared.
    pub fn set_open_group(&mut self, app_key: Option<String>) {
        if let Some(k) = app_key {
            if self.groups.contains_key(&k) {
                self.open_group = Some(k);
            } else {
                self.open_group = None;
            }
        } else {
            self.open_group = None;
        }
    }

    pub fn toggle_open_group(&mut self, app_key: &str) {
        let k = app_key.to_string();
        if self.open_group.as_deref() == Some(app_key) {
            self.open_group = None;
        } else if self.groups.contains_key(app_key) {
            self.open_group = Some(k);
        } else {
            self.open_group = None;
        }
    }

    // Method is used exclusively to verify logic in tests
    #[cfg_attr(not(test), allow(dead_code))]
    pub fn open_group(&self) -> Option<&str> {
        self.open_group.as_deref()
    }

    /// Returns a sorted snapshot for rendering.
    pub fn snapshot(&self) -> NotificationsSnapshot {
        let mut groups: Vec<NotificationGroup> = self.groups.values().cloned().collect();

        groups.sort_by(|a, b| match (a.latest_ts(), b.latest_ts()) {
            (Some(ta), Some(tb)) => systemtime_cmp_desc(&ta, &tb),
            (Some(_), None) => std::cmp::Ordering::Less,
            (None, Some(_)) => std::cmp::Ordering::Greater,
            (None, None) => std::cmp::Ordering::Equal,
        });

        let total_count = groups.iter().map(|g| g.notifications.len()).sum();

        NotificationsSnapshot {
            groups,
            open_group: self.open_group.clone(),
            total_count,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::features::notifications::types::NotificationIcon;

    fn t(secs: u64) -> SystemTime {
        std::time::UNIX_EPOCH + std::time::Duration::from_secs(secs)
    }

    fn n(id: u64, app: &str, ts: u64) -> Notification {
        Notification::new(
            id,
            app.to_string(),
            format!("s{id}"),
            format!("b{id}"),
            t(ts),
            NotificationIcon::Themed("dialog-information-symbolic".to_string()),
        )
    }

    #[test]
    fn groups_by_normalized_app_name() {
        let mut m = NotificationsModel::new();
        m.add(n(1, "Slack", 10));
        m.add(n(2, "slack", 11));
        m.add(n(3, "SLACK ", 12));
        m.add(n(4, "org.example.App", 13));

        let snap = m.snapshot();
        // "Slack"/"slack"/"SLACK " all normalize to "slack"
        assert_eq!(snap.groups.len(), 2);

        let keys: Vec<String> = snap.groups.iter().map(|g| g.app_key.clone()).collect();
        assert!(keys.contains(&"slack".to_string()));
        assert!(keys.contains(&"org.example.app".to_string()));
    }

    #[test]
    fn notifications_sorted_newest_first_within_group() {
        let mut m = NotificationsModel::new();
        m.add(n(1, "Slack", 10));
        m.add(n(2, "Slack", 12));
        m.add(n(3, "Slack", 11));

        let snap = m.snapshot();
        let g = snap.groups.iter().find(|g| g.app_key == "slack").unwrap();

        let ids: Vec<u64> = g.notifications.iter().map(|n| n.id).collect();
        assert_eq!(ids, vec![2, 3, 1]);
    }

    #[test]
    fn groups_sorted_by_latest_notification_newest_first() {
        let mut m = NotificationsModel::new();
        m.add(n(1, "AppA", 10));
        m.add(n(2, "AppB", 20));
        m.add(n(3, "AppA", 30)); // AppA becomes newest group

        let snap = m.snapshot();
        assert_eq!(snap.groups.len(), 2);
        assert_eq!(snap.groups[0].app_key, "appa");
        assert_eq!(snap.groups[1].app_key, "appb");
    }

    #[test]
    fn only_one_group_open_at_a_time() {
        let mut m = NotificationsModel::new();
        m.add(n(1, "AppA", 10));
        m.add(n(2, "AppB", 20));

        m.set_open_group(Some("appa".to_string()));
        assert_eq!(m.open_group(), Some("appa"));

        m.set_open_group(Some("appb".to_string()));
        assert_eq!(m.open_group(), Some("appb"));
    }

    #[test]
    fn removing_latest_reveals_next_latest_and_group_disappears_when_empty() {
        let mut m = NotificationsModel::new();
        m.add(n(1, "Slack", 10));
        m.add(n(2, "Slack", 20)); // latest
        m.add(n(3, "Slack", 15));

        // Remove latest (id=2), next latest should be id=3
        assert!(m.remove(2));
        let snap = m.snapshot();
        let g = snap.groups.iter().find(|g| g.app_key == "slack").unwrap();
        assert_eq!(g.notifications[0].id, 3);

        // Remove remaining
        assert!(m.remove(3));
        assert!(m.remove(1));
        let snap = m.snapshot();
        assert!(snap.groups.is_empty());
        assert_eq!(snap.total_count, 0);
    }

    #[test]
    fn clear_removes_everything_and_closes_open_group() {
        let mut m = NotificationsModel::new();
        m.add(n(1, "Slack", 10));
        m.add(n(2, "AppB", 20));
        m.set_open_group(Some("slack".to_string()));

        m.clear();
        let snap = m.snapshot();
        assert!(snap.groups.is_empty());
        assert_eq!(snap.total_count, 0);
        assert_eq!(snap.open_group, None);
    }
}
