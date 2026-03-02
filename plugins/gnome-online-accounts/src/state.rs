//! GOA account state storage.

use std::collections::HashMap;

use waft_protocol::entity::accounts::{ONLINE_ACCOUNT_ENTITY_TYPE, OnlineAccount};
use waft_plugin::{Entity, Urn};

/// Internal state tracking all GOA accounts.
#[derive(Debug, Default)]
pub struct GoaState {
    /// Accounts keyed by GOA `Id` property.
    pub accounts: HashMap<String, OnlineAccount>,
    /// Maps account ID to D-Bus object path.
    pub paths: HashMap<String, String>,
}

impl GoaState {
    /// Insert or update an account and its D-Bus path.
    pub fn update_account(&mut self, id: String, path: String, account: OnlineAccount) {
        self.accounts.insert(id.clone(), account);
        self.paths.insert(id, path);
    }

    /// Remove an account by its ID.
    pub fn remove_account(&mut self, id: &str) {
        self.accounts.remove(id);
        self.paths.remove(id);
    }

    /// Remove an account by its D-Bus object path, returning the removed ID if found.
    pub fn remove_by_path(&mut self, path: &str) -> Option<String> {
        let id = self
            .paths
            .iter()
            .find(|(_, p)| p.as_str() == path)
            .map(|(id, _)| id.clone());

        if let Some(ref id) = id {
            self.accounts.remove(id);
            self.paths.remove(id);
        }

        id
    }

    /// Find the account ID associated with a D-Bus object path.
    pub fn id_for_path(&self, path: &str) -> Option<&str> {
        self.paths
            .iter()
            .find(|(_, p)| p.as_str() == path)
            .map(|(id, _)| id.as_str())
    }

    /// Get the D-Bus object path for an account ID.
    pub fn object_path_for_id(&self, id: &str) -> Option<&str> {
        self.paths.get(id).map(|p| p.as_str())
    }

    /// Convert all accounts to protocol entities.
    pub fn get_entities(&self) -> Vec<Entity> {
        let mut entities: Vec<_> = self
            .accounts
            .iter()
            .map(|(id, account)| {
                let urn =
                    Urn::new("gnome-online-accounts", ONLINE_ACCOUNT_ENTITY_TYPE, id);
                Entity::new(urn, ONLINE_ACCOUNT_ENTITY_TYPE, account)
            })
            .collect();

        // Sort by ID for stable ordering
        entities.sort_by(|a, b| a.urn.to_string().cmp(&b.urn.to_string()));
        entities
    }
}
