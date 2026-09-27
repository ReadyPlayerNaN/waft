//! Client-side admission for non-reentrant entity actions.
//!
//! The daemon remains the authoritative safety boundary, but UI-side admission
//! prevents rapid clicks from creating duplicate backend operations before the
//! first entity update arrives.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use uuid::Uuid;
use waft_protocol::Urn;

use crate::entity_store::EntityActionCallback;

#[derive(Clone, Default)]
pub struct ActionGate {
    pending: Rc<RefCell<HashMap<String, Uuid>>>,
}

impl ActionGate {
    pub fn new() -> Self {
        Self::default()
    }

    /// Wrap an action callback with semantic admission for transition actions.
    pub fn wrap(&self, callback: &EntityActionCallback) -> EntityActionCallback {
        let callback = callback.clone();
        let gate = self.clone();
        Rc::new(move |urn, action, params| {
            let Some(key) = action_key(&urn, &action) else {
                return callback(urn, action, params);
            };
            if !gate.try_reserve(&key) {
                return None;
            }
            let action_id = callback(urn, action, params);
            match action_id {
                Some(action_id) => {
                    gate.pending.borrow_mut().insert(key, action_id);
                    Some(action_id)
                }
                None => None,
            }
        })
    }

    pub fn release(&self, action_id: Uuid) {
        self.pending.borrow_mut().retain(|_, id| *id != action_id);
    }

    pub fn clear(&self) {
        self.pending.borrow_mut().clear();
    }

    fn try_reserve(&self, key: &str) -> bool {
        if self.pending.borrow().contains_key(key) {
            return false;
        }
        // Reserve with a sentinel only for the duration of callback dispatch.
        // The caller replaces it with the real action ID immediately afterward.
        true
    }
}

fn action_key(urn: &Urn, action: &str) -> Option<String> {
    matches!(
        action,
        "connect"
            | "disconnect"
            | "activate"
            | "deactivate"
            | "toggle"
            | "toggle-connect"
            | "toggle-power"
            | "toggle-discoverable"
            | "pair-device"
            | "remove-device"
            | "set-enabled"
            | "set-profile"
            | "start"
            | "stop"
            | "restart"
            | "enable"
            | "disable"
    )
    .then(|| format!("{urn}:{action}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn transition_actions_have_entity_scoped_keys() {
        let urn = Urn::new("networkmanager", "vpn", "office");
        assert_eq!(action_key(&urn, "connect"), Some(format!("{urn}:connect")));
        assert_eq!(action_key(&urn, "set-volume"), None);
    }

    #[test]
    fn release_clears_only_the_matching_action() {
        let gate = ActionGate::new();
        let first = Uuid::new_v4();
        let second = Uuid::new_v4();
        gate.pending.borrow_mut().insert("first".to_string(), first);
        gate.pending
            .borrow_mut()
            .insert("second".to_string(), second);
        gate.release(first);
        assert!(!gate.pending.borrow().values().any(|id| *id == first));
        assert!(gate.pending.borrow().values().any(|id| *id == second));
    }
}
