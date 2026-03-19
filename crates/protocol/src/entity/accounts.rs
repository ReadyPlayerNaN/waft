use serde::{Deserialize, Serialize};

/// Entity type identifier for GNOME Online Accounts.
pub const ONLINE_ACCOUNT_ENTITY_TYPE: &str = "online-account";

/// Entity type identifier for online account providers.
pub const ONLINE_ACCOUNT_PROVIDER_ENTITY_TYPE: &str = "online-account-provider";

/// Account health status derived from GOA D-Bus properties.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum AccountStatus {
    /// Account is working normally.
    Active,
    /// Credentials have expired or are invalid; re-authentication needed.
    CredentialsNeeded,
    /// Account requires user attention for a non-credential issue.
    NeedsAttention,
}

/// A single service toggle on an online account.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ServiceInfo {
    /// Service identifier matching the GOA D-Bus property prefix
    /// (e.g. "mail", "calendar", "contacts", "files", "photos", "ticketing", "chat", "music").
    pub name: String,
    /// Whether this service is currently enabled.
    pub enabled: bool,
}

/// An available online account provider (e.g. Google, Microsoft 365).
///
/// URN: `gnome-online-accounts/online-account-provider/{provider-type}`
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OnlineAccountProvider {
    /// Provider type identifier (e.g. "google", "ms365", "owncloud").
    pub provider_type: String,
    /// Human-readable display name (e.g. "Google", "Microsoft 365").
    pub provider_name: String,
    /// Themed icon name for the provider.
    pub icon_name: Option<String>,
}

impl OnlineAccountProvider {
    /// Entity type identifier for online account providers.
    pub const ENTITY_TYPE: &str = ONLINE_ACCOUNT_PROVIDER_ENTITY_TYPE;
}

/// A GNOME Online Account.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OnlineAccount {
    /// GOA account ID (e.g. "account_1234567890").
    pub id: String,
    /// Provider display name (e.g. "Google", "Nextcloud").
    pub provider_name: String,
    /// User-facing account identity (e.g. "user@gmail.com").
    pub presentation_identity: String,
    /// Account health status.
    pub status: AccountStatus,
    /// Per-service enabled/disabled state.
    /// Only includes services that the provider actually supports.
    pub services: Vec<ServiceInfo>,
    /// Whether the account is administrator-locked (removal discouraged).
    pub locked: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn online_account_serde_roundtrip_active() {
        let account = OnlineAccount {
            id: "account_1234567890".to_string(),
            provider_name: "Google".to_string(),
            presentation_identity: "user@gmail.com".to_string(),
            status: AccountStatus::Active,
            services: vec![
                ServiceInfo { name: "mail".to_string(), enabled: true },
                ServiceInfo { name: "calendar".to_string(), enabled: true },
                ServiceInfo { name: "contacts".to_string(), enabled: false },
            ],
            locked: false,
        };
        let json = serde_json::to_value(&account).unwrap();
        let decoded: OnlineAccount = serde_json::from_value(json).unwrap();
        assert_eq!(account, decoded);
    }

    #[test]
    fn online_account_serde_roundtrip_credentials_needed_empty_services() {
        let account = OnlineAccount {
            id: "account_9999".to_string(),
            provider_name: "Microsoft".to_string(),
            presentation_identity: "user@outlook.com".to_string(),
            status: AccountStatus::CredentialsNeeded,
            services: vec![],
            locked: false,
        };
        let json = serde_json::to_value(&account).unwrap();
        let decoded: OnlineAccount = serde_json::from_value(json).unwrap();
        assert_eq!(account, decoded);
    }

    #[test]
    fn online_account_serde_roundtrip_needs_attention_locked() {
        let account = OnlineAccount {
            id: "account_locked".to_string(),
            provider_name: "Nextcloud".to_string(),
            presentation_identity: "admin@company.example".to_string(),
            status: AccountStatus::NeedsAttention,
            services: vec![
                ServiceInfo { name: "files".to_string(), enabled: true },
            ],
            locked: true,
        };
        let json = serde_json::to_value(&account).unwrap();
        let decoded: OnlineAccount = serde_json::from_value(json).unwrap();
        assert_eq!(account, decoded);
    }

    #[test]
    fn service_info_serde_roundtrip() {
        let service = ServiceInfo {
            name: "calendar".to_string(),
            enabled: true,
        };
        let json = serde_json::to_value(&service).unwrap();
        let decoded: ServiceInfo = serde_json::from_value(json).unwrap();
        assert_eq!(service, decoded);
    }

    #[test]
    fn account_status_serde_roundtrip_all_variants() {
        for status in [
            AccountStatus::Active,
            AccountStatus::CredentialsNeeded,
            AccountStatus::NeedsAttention,
        ] {
            let json = serde_json::to_value(&status).unwrap();
            let decoded: AccountStatus = serde_json::from_value(json).unwrap();
            assert_eq!(status, decoded);
        }
    }

    #[test]
    fn online_account_provider_serde_roundtrip() {
        let provider = OnlineAccountProvider {
            provider_type: "google".to_string(),
            provider_name: "Google".to_string(),
            icon_name: Some("goa-account-google".to_string()),
        };
        let json = serde_json::to_value(&provider).unwrap();
        let decoded: OnlineAccountProvider = serde_json::from_value(json).unwrap();
        assert_eq!(provider, decoded);
    }

    #[test]
    fn online_account_provider_serde_roundtrip_no_icon() {
        let provider = OnlineAccountProvider {
            provider_type: "owncloud".to_string(),
            provider_name: "ownCloud".to_string(),
            icon_name: None,
        };
        let json = serde_json::to_value(&provider).unwrap();
        let decoded: OnlineAccountProvider = serde_json::from_value(json).unwrap();
        assert_eq!(provider, decoded);
    }
}
