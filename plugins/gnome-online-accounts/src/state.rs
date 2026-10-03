//! GOA account state storage.

use std::collections::HashMap;

use waft_plugin::{Entity, Urn};
use waft_protocol::entity::accounts::{
    ONLINE_ACCOUNT_ENTITY_TYPE, ONLINE_ACCOUNT_PROVIDER_ENTITY_TYPE,
    ONLINE_ACCOUNTS_STATUS_ENTITY_TYPE, OnlineAccount, OnlineAccountProvider, OnlineAccountsStatus,
};

/// Internal state tracking all GOA accounts.
#[derive(Debug, Default)]
pub struct GoaState {
    /// Accounts keyed by GOA `Id` property.
    pub accounts: HashMap<String, OnlineAccount>,
    /// Maps account ID to D-Bus object path.
    pub paths: HashMap<String, String>,
    /// Available GOA providers.
    pub providers: Vec<OnlineAccountProvider>,
    pub status: OnlineAccountsStatus,
    pub(crate) session: Option<crate::lifecycle::Session>,
    pub(crate) revision: u64,
    pub(crate) committed_revision: u64,
    pub(crate) announced_owner: Option<String>,
    pub(crate) generation: u64,
    pub(crate) incarnations: HashMap<String, u64>,
    pub(crate) replacements: HashMap<String, u64>,
    pub(crate) identities: HashMap<String, String>,
    pub(crate) unsupported_providers: std::collections::BTreeSet<String>,
    pub(crate) capabilities: crate::dbus::CapabilityHistory,
    pub(crate) busy: std::collections::HashSet<String>,
    pub(crate) launch_targets: std::collections::HashSet<String>,
}

impl GoaState {
    pub(crate) fn dependency_lost(&mut self) {
        use waft_protocol::entity::accounts::Availability;
        self.generation += 1;
        self.session = None;
        self.status.accounts = if self.status.last_accounts_snapshot.is_some() {
            Availability::Recovering
        } else {
            Availability::Unavailable
        };
        self.status.providers = if self.status.last_providers_snapshot.is_some() {
            Availability::Recovering
        } else {
            Availability::Unavailable
        };
    }

    pub(crate) fn replace_accounts(
        &mut self,
        snapshot: crate::dbus::AccountSnapshot,
        capabilities: crate::dbus::CapabilityHistory,
        identities: HashMap<String, String>,
    ) {
        let next: std::collections::HashSet<_> =
            snapshot.iter().map(|(id, _, _)| id.clone()).collect();
        for id in self.accounts.keys().filter(|id| !next.contains(*id)) {
            *self.incarnations.entry(id.clone()).or_default() += 1;
        }
        for (id, path, account) in &snapshot {
            if self.paths.get(id).is_some_and(|old| old != path)
                || self.accounts.get(id).is_some_and(|old| {
                    old.provider_type != account.provider_type
                        || old.presentation_identity != account.presentation_identity
                })
                || self
                    .identities
                    .get(id)
                    .is_some_and(|old| identities.get(id) != Some(old))
            {
                *self.incarnations.entry(id.clone()).or_default() += 1;
                *self.replacements.entry(id.clone()).or_default() += 1;
            }
        }
        self.identities = identities;
        self.accounts.clear();
        self.paths.clear();
        for (id, path, account) in snapshot {
            self.update_account(id, path, account);
        }
        self.capabilities = capabilities;
    }

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
        self.paths.get(id).map(std::string::String::as_str)
    }

    /// Convert all accounts and providers to protocol entities.
    pub fn get_entities(&self) -> Vec<Entity> {
        let mut entities: Vec<_> = self
            .accounts
            .iter()
            .map(|(id, account)| {
                let urn = Urn::new("gnome-online-accounts", ONLINE_ACCOUNT_ENTITY_TYPE, id);
                Entity::new(urn, ONLINE_ACCOUNT_ENTITY_TYPE, account)
            })
            .collect();

        // Add provider entities
        for provider in &self.providers {
            let urn = Urn::new(
                "gnome-online-accounts",
                ONLINE_ACCOUNT_PROVIDER_ENTITY_TYPE,
                &provider.provider_type,
            );
            entities.push(Entity::new(
                urn,
                ONLINE_ACCOUNT_PROVIDER_ENTITY_TYPE,
                provider,
            ));
        }

        entities.push(Entity::new(
            Urn::new(
                "gnome-online-accounts",
                ONLINE_ACCOUNTS_STATUS_ENTITY_TYPE,
                "singleton",
            ),
            ONLINE_ACCOUNTS_STATUS_ENTITY_TYPE,
            &self.status,
        ));

        // Sort by URN for stable ordering
        entities.sort_by_key(|a| a.urn.to_string());
        entities
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use waft_protocol::entity::accounts::{AccountStatus, ServiceInfo};

    fn test_account(id: &str, provider: &str) -> OnlineAccount {
        OnlineAccount {
            id: id.to_string(),
            provider_name: provider.to_string(),
            provider_type: String::new(),
            presentation_identity: format!("{id}@example.com"),
            status: AccountStatus::Active,
            services: vec![ServiceInfo {
                name: "mail".to_string(),
                enabled: true,
            }],
            locked: false,
        }
    }

    #[test]
    fn update_and_get_account() {
        let mut state = GoaState::default();
        let account = test_account("acc1", "Google");
        state.update_account(
            "acc1".into(),
            "/org/gnome/OnlineAccounts/Accounts/acc1".into(),
            account.clone(),
        );

        assert_eq!(state.accounts.get("acc1"), Some(&account));
        assert_eq!(
            state.object_path_for_id("acc1"),
            Some("/org/gnome/OnlineAccounts/Accounts/acc1")
        );
    }

    #[test]
    fn remove_account_by_id() {
        let mut state = GoaState::default();
        state.update_account(
            "acc1".into(),
            "/path/acc1".into(),
            test_account("acc1", "Google"),
        );
        state.update_account(
            "acc2".into(),
            "/path/acc2".into(),
            test_account("acc2", "Microsoft"),
        );

        state.remove_account("acc1");
        assert!(!state.accounts.contains_key("acc1"));
        assert!(!state.paths.contains_key("acc1"));
        assert!(state.accounts.contains_key("acc2"));
    }

    #[test]
    fn remove_by_path_returns_id() {
        let mut state = GoaState::default();
        state.update_account(
            "acc1".into(),
            "/path/acc1".into(),
            test_account("acc1", "Google"),
        );

        let removed = state.remove_by_path("/path/acc1");
        assert_eq!(removed, Some("acc1".to_string()));
        assert!(state.accounts.is_empty());
        assert!(state.paths.is_empty());
    }

    #[test]
    fn remove_by_path_unknown_returns_none() {
        let mut state = GoaState::default();
        state.update_account(
            "acc1".into(),
            "/path/acc1".into(),
            test_account("acc1", "Google"),
        );

        let removed = state.remove_by_path("/path/nonexistent");
        assert_eq!(removed, None);
        assert_eq!(state.accounts.len(), 1);
    }

    #[test]
    fn id_for_path_lookup() {
        let mut state = GoaState::default();
        state.update_account(
            "acc1".into(),
            "/path/acc1".into(),
            test_account("acc1", "Google"),
        );

        assert_eq!(state.id_for_path("/path/acc1"), Some("acc1"));
        assert_eq!(state.id_for_path("/path/unknown"), None);
    }

    #[test]
    fn get_entities_includes_accounts_and_providers() {
        let mut state = GoaState::default();
        state.update_account(
            "acc1".into(),
            "/path/acc1".into(),
            test_account("acc1", "Google"),
        );
        state.providers = vec![OnlineAccountProvider {
            provider_type: "google".to_string(),
            provider_name: "Google".to_string(),
            icon_name: Some("goa-account-google".to_string()),
        }];

        let entities = state.get_entities();
        assert_eq!(entities.len(), 3);

        // Entities are sorted by URN string
        let entity_types: Vec<&str> = entities.iter().map(|e| e.entity_type.as_str()).collect();
        assert!(entity_types.contains(&ONLINE_ACCOUNT_ENTITY_TYPE));
        assert!(entity_types.contains(&ONLINE_ACCOUNT_PROVIDER_ENTITY_TYPE));
    }

    #[test]
    fn get_entities_empty_state() {
        let state = GoaState::default();
        assert_eq!(state.get_entities().len(), 1);
    }

    #[test]
    fn update_account_overwrites_existing() {
        let mut state = GoaState::default();
        state.update_account(
            "acc1".into(),
            "/path/acc1".into(),
            test_account("acc1", "Google"),
        );

        let updated = OnlineAccount {
            id: "acc1".to_string(),
            provider_name: "Google".to_string(),
            provider_type: "google".to_string(),
            presentation_identity: "new@gmail.com".to_string(),
            status: AccountStatus::CredentialsNeeded,
            services: vec![],
            locked: false,
        };
        state.update_account("acc1".into(), "/new/path".into(), updated.clone());

        assert_eq!(state.accounts.get("acc1"), Some(&updated));
        assert_eq!(state.object_path_for_id("acc1"), Some("/new/path"));
        assert_eq!(state.accounts.len(), 1);
    }
}
