use super::super::types::NotificationAction;
use std::sync::Arc;

pub fn parse_actions(actions_raw: Vec<String>, _use_icons: bool) -> Vec<NotificationAction> {
    // Spec: alternating action_key, label.
    let mut out = Vec::new();
    let mut it = actions_raw.into_iter();
    loop {
        let Some(key) = it.next() else { break };
        let Some(label) = it.next() else { break };
        out.push(NotificationAction {
            key: Arc::from(key),
            label: Arc::from(label),
            icon: None,
        });
    }
    out
}
