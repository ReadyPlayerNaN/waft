//! GOA D-Bus constants, property helpers, and operations.

use std::collections::HashMap;

use anyhow::{Context, Result};
use log::{debug, warn};
use zbus::Connection;
use zbus::zvariant::OwnedValue;

use waft_protocol::entity::accounts::{
    AccountStatus, OnlineAccount, OnlineAccountProvider, ServiceInfo,
};

// ---------------------------------------------------------------------------
// D-Bus constants
// ---------------------------------------------------------------------------

pub const GOA_BUS_NAME: &str = "org.gnome.OnlineAccounts";
pub const GOA_OBJECT_PATH: &str = "/org/gnome/OnlineAccounts";
pub const GOA_MANAGER_PATH: &str = "/org/gnome/OnlineAccounts/Manager";
pub const GOA_MANAGER_IFACE: &str = "org.gnome.OnlineAccounts.Manager";
pub const GOA_ACCOUNT_IFACE: &str = "org.gnome.OnlineAccounts.Account";
pub const IFACE_OBJECT_MANAGER: &str = "org.freedesktop.DBus.ObjectManager";
pub const IFACE_PROPERTIES: &str = "org.freedesktop.DBus.Properties";

/// Known GOA service types and their D-Bus property prefixes.
///
/// Each entry maps `(CapitalizedServiceName, lowercase_service_id)`.
/// The D-Bus property for each service is `{CapitalizedServiceName}Disabled`.
pub const KNOWN_SERVICES: &[(&str, &str)] = &[
    ("Mail", "mail"),
    ("Calendar", "calendar"),
    ("Contacts", "contacts"),
    ("Chat", "chat"),
    ("Files", "files"),
    ("Music", "music"),
    ("Photos", "photos"),
    ("Ticketing", "ticketing"),
];

// ---------------------------------------------------------------------------
// ManagedObjects type alias
// ---------------------------------------------------------------------------

pub type ManagedObjects =
    HashMap<zbus::zvariant::OwnedObjectPath, HashMap<String, HashMap<String, OwnedValue>>>;

// ---------------------------------------------------------------------------
// Property extraction
// ---------------------------------------------------------------------------

fn extract_string(
    props: &HashMap<String, OwnedValue>,
    key: &str,
    default: &str,
) -> String {
    props
        .get(key)
        .and_then(|v| String::try_from(v.clone()).ok())
        .unwrap_or_else(|| default.to_string())
}

fn extract_bool(props: &HashMap<String, OwnedValue>, key: &str, default: bool) -> bool {
    props
        .get(key)
        .and_then(|v| bool::try_from(v.clone()).ok())
        .unwrap_or(default)
}

/// Map GOA `AttentionNeeded` property to our status enum.
///
/// GOA exposes `AttentionNeeded: bool` on the Account interface. When true,
/// the account needs user action. We map:
/// - `false` -> `Active`
/// - `true`  -> `NeedsAttention` (the caller can refine to `CredentialsNeeded`
///   if appropriate, but initial discovery uses this simple mapping).
pub fn parse_account_status(attention_needed: bool) -> AccountStatus {
    if attention_needed {
        AccountStatus::NeedsAttention
    } else {
        AccountStatus::Active
    }
}

/// Map a service identifier to its GOA D-Bus `*Disabled` property name.
///
/// Returns `None` if the service name is not recognized.
pub fn service_name_to_property(name: &str) -> Option<String> {
    KNOWN_SERVICES
        .iter()
        .find(|(_cap, id)| *id == name)
        .map(|(cap, _)| format!("{cap}Disabled"))
}

/// Parse the list of services an account supports from its D-Bus properties.
///
/// Only includes services whose `*Disabled` property exists on the object.
fn parse_services(props: &HashMap<String, OwnedValue>) -> Vec<ServiceInfo> {
    let mut services = Vec::new();
    for (capitalized, service_id) in KNOWN_SERVICES {
        let prop_name = format!("{capitalized}Disabled");
        if let Some(val) = props.get(&prop_name) {
            let disabled = bool::try_from(val.clone()).unwrap_or(true);
            services.push(ServiceInfo {
                name: service_id.to_string(),
                enabled: !disabled,
            });
        }
    }
    services
}

/// Parse a single GOA account from its D-Bus properties.
///
/// Returns `(id, OnlineAccount)` or `None` if `Id` is missing.
pub fn parse_account(props: &HashMap<String, OwnedValue>) -> Option<(String, OnlineAccount)> {
    let id = props
        .get("Id")
        .and_then(|v| String::try_from(v.clone()).ok())?;

    let provider_name = extract_string(props, "ProviderName", "Unknown");
    let presentation_identity = extract_string(props, "PresentationIdentity", "");
    let attention_needed = extract_bool(props, "AttentionNeeded", false);
    let locked = extract_bool(props, "IsLocked", false);
    let status = parse_account_status(attention_needed);
    let services = parse_services(props);

    Some((
        id.clone(),
        OnlineAccount {
            id,
            provider_name,
            presentation_identity,
            status,
            services,
            locked,
        },
    ))
}

// ---------------------------------------------------------------------------
// D-Bus operations
// ---------------------------------------------------------------------------

/// Call `GetManagedObjects` on GOA and return all accounts.
///
/// Returns a vec of `(account_id, object_path, OnlineAccount)`.
pub async fn discover_accounts(
    conn: &Connection,
) -> Result<Vec<(String, String, OnlineAccount)>> {
    let proxy = zbus::Proxy::new(conn, GOA_BUS_NAME, GOA_OBJECT_PATH, IFACE_OBJECT_MANAGER)
        .await
        .context("Failed to create GOA ObjectManager proxy")?;

    let (objects,): (ManagedObjects,) = proxy
        .call("GetManagedObjects", &())
        .await
        .context("Failed to call GetManagedObjects on GOA")?;

    let mut accounts = Vec::new();

    for (path, interfaces) in &objects {
        let Some(account_props) = interfaces.get(GOA_ACCOUNT_IFACE) else {
            continue;
        };

        let path_str = path.to_string();

        match parse_account(account_props) {
            Some((id, account)) => {
                debug!("[goa] Discovered account: {} ({})", id, account.provider_name);
                accounts.push((id, path_str, account));
            }
            None => {
                warn!("[goa] Account object at {} missing Id property, skipping", path_str);
            }
        }
    }

    // Sort by ID for stable ordering
    accounts.sort_by(|a, b| a.0.cmp(&b.0));

    Ok(accounts)
}

/// Set a service's disabled state on a GOA account via `Properties.Set`.
pub async fn set_service_disabled(
    conn: &Connection,
    account_path: &str,
    service_name: &str,
    disabled: bool,
) -> Result<()> {
    let prop_name = service_name_to_property(service_name)
        .context(format!("Unknown service name: {service_name}"))?;

    let proxy = zbus::Proxy::new(conn, GOA_BUS_NAME, account_path, IFACE_PROPERTIES)
        .await
        .context("Failed to create Properties proxy")?;

    let v = zbus::zvariant::Value::from(disabled);
    let _: () = proxy
        .call("Set", &(GOA_ACCOUNT_IFACE, &prop_name, v))
        .await
        .context(format!(
            "Failed to set {prop_name} on {account_path}"
        ))?;

    debug!(
        "[goa] Set {} = {} on {}",
        prop_name, disabled, account_path
    );

    Ok(())
}

/// Remove a GOA account via the `Account.Remove()` D-Bus method.
pub async fn remove_account(conn: &Connection, account_path: &str) -> Result<()> {
    let proxy = zbus::Proxy::new(conn, GOA_BUS_NAME, account_path, GOA_ACCOUNT_IFACE)
        .await
        .context("Failed to create Account proxy for removal")?;
    let _: () = proxy
        .call("Remove", &())
        .await
        .context(format!("Failed to call Remove on {account_path}"))?;
    debug!("[goa] Called Remove on {}", account_path);
    Ok(())
}

// ---------------------------------------------------------------------------
// Provider enumeration
// ---------------------------------------------------------------------------

/// Known GOA provider types with display names and icon names.
///
/// These are the standard providers shipped with gnome-online-accounts.
/// We check each via `Manager.IsSupportedProvider` to determine availability.
const KNOWN_PROVIDERS: &[(&str, &str, &str)] = &[
    ("google", "Google", "goa-account-google"),
    ("ms365", "Microsoft 365", "goa-account-msn"),
    ("owncloud", "Nextcloud", "goa-account-owncloud"),
    ("imap_smtp", "IMAP and SMTP", "goa-account-imap-smtp"),
    ("exchange", "Microsoft Exchange", "goa-account-exchange"),
    ("kerberos", "Enterprise Login (Kerberos)", "goa-account-kerberos"),
    ("fedora", "Fedora", "goa-account-fedora"),
    ("webdav", "WebDAV", "goa-account-webdav"),
];

/// Check if a GOA provider type is supported via `Manager.IsSupportedProvider`.
async fn is_supported_provider(conn: &Connection, provider_type: &str) -> bool {
    let proxy = match zbus::Proxy::new(conn, GOA_BUS_NAME, GOA_MANAGER_PATH, GOA_MANAGER_IFACE)
        .await
    {
        Ok(p) => p,
        Err(_) => return false,
    };

    match proxy
        .call::<_, _, (bool,)>("IsSupportedProvider", &(provider_type.to_string(),))
        .await
    {
        Ok((supported,)) => supported,
        Err(e) => {
            debug!(
                "[goa] IsSupportedProvider({}) failed: {}",
                provider_type, e
            );
            false
        }
    }
}

/// Discover available GOA providers via D-Bus.
///
/// Checks each known provider type with `Manager.IsSupportedProvider` and
/// returns the supported ones as `OnlineAccountProvider` structs.
/// Returns an empty vec if goa-daemon is not running.
pub async fn discover_providers(conn: &Connection) -> Vec<OnlineAccountProvider> {
    let mut providers = Vec::new();

    for &(provider_type, name, icon) in KNOWN_PROVIDERS {
        if is_supported_provider(conn, provider_type).await {
            providers.push(OnlineAccountProvider {
                provider_type: provider_type.to_string(),
                provider_name: name.to_string(),
                icon_name: Some(icon.to_string()),
            });
        }
    }

    providers
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_account_status_active() {
        assert_eq!(parse_account_status(false), AccountStatus::Active);
    }

    #[test]
    fn parse_account_status_needs_attention() {
        assert_eq!(parse_account_status(true), AccountStatus::NeedsAttention);
    }

    #[test]
    fn service_name_to_property_known() {
        assert_eq!(
            service_name_to_property("mail"),
            Some("MailDisabled".to_string())
        );
        assert_eq!(
            service_name_to_property("calendar"),
            Some("CalendarDisabled".to_string())
        );
        assert_eq!(
            service_name_to_property("ticketing"),
            Some("TicketingDisabled".to_string())
        );
    }

    #[test]
    fn service_name_to_property_unknown() {
        assert_eq!(service_name_to_property("nonexistent"), None);
    }

    #[test]
    fn service_name_to_property_all_known_services() {
        // Verify all 8 known services map correctly
        let expected = [
            ("mail", "MailDisabled"),
            ("calendar", "CalendarDisabled"),
            ("contacts", "ContactsDisabled"),
            ("chat", "ChatDisabled"),
            ("files", "FilesDisabled"),
            ("music", "MusicDisabled"),
            ("photos", "PhotosDisabled"),
            ("ticketing", "TicketingDisabled"),
        ];
        for (service, prop) in expected {
            assert_eq!(
                service_name_to_property(service),
                Some(prop.to_string()),
                "Failed for service: {}",
                service
            );
        }
    }

    fn make_props(entries: &[(&str, OwnedValue)]) -> HashMap<String, OwnedValue> {
        entries
            .iter()
            .map(|(k, v)| (k.to_string(), v.clone()))
            .collect()
    }

    fn string_val(s: &str) -> OwnedValue {
        OwnedValue::try_from(zbus::zvariant::Value::from(s.to_string())).unwrap()
    }

    fn bool_val(b: bool) -> OwnedValue {
        OwnedValue::try_from(zbus::zvariant::Value::from(b)).unwrap()
    }

    #[test]
    fn parse_services_extracts_known_disabled_props() {
        let props = make_props(&[
            ("MailDisabled", bool_val(false)),
            ("CalendarDisabled", bool_val(true)),
        ]);
        let services = parse_services(&props);
        assert_eq!(services.len(), 2);
        assert_eq!(services[0].name, "mail");
        assert!(services[0].enabled); // MailDisabled=false -> enabled=true
        assert_eq!(services[1].name, "calendar");
        assert!(!services[1].enabled); // CalendarDisabled=true -> enabled=false
    }

    #[test]
    fn parse_services_empty_props() {
        let props = HashMap::new();
        let services = parse_services(&props);
        assert!(services.is_empty());
    }

    #[test]
    fn parse_services_ignores_unknown_props() {
        let props = make_props(&[
            ("SomethingDisabled", bool_val(false)),
            ("MailDisabled", bool_val(false)),
        ]);
        let services = parse_services(&props);
        assert_eq!(services.len(), 1);
        assert_eq!(services[0].name, "mail");
    }

    #[test]
    fn parse_account_complete() {
        let props = make_props(&[
            ("Id", string_val("account_123")),
            ("ProviderName", string_val("Google")),
            ("PresentationIdentity", string_val("user@gmail.com")),
            ("AttentionNeeded", bool_val(false)),
            ("IsLocked", bool_val(false)),
            ("MailDisabled", bool_val(false)),
            ("CalendarDisabled", bool_val(true)),
        ]);
        let (id, account) = parse_account(&props).unwrap();
        assert_eq!(id, "account_123");
        assert_eq!(account.provider_name, "Google");
        assert_eq!(account.presentation_identity, "user@gmail.com");
        assert_eq!(account.status, AccountStatus::Active);
        assert!(!account.locked);
        assert_eq!(account.services.len(), 2);
    }

    #[test]
    fn parse_account_attention_needed_and_locked() {
        let props = make_props(&[
            ("Id", string_val("account_456")),
            ("ProviderName", string_val("Nextcloud")),
            ("PresentationIdentity", string_val("admin@company.example")),
            ("AttentionNeeded", bool_val(true)),
            ("IsLocked", bool_val(true)),
        ]);
        let (id, account) = parse_account(&props).unwrap();
        assert_eq!(id, "account_456");
        assert_eq!(account.status, AccountStatus::NeedsAttention);
        assert!(account.locked);
    }

    #[test]
    fn parse_account_missing_id_returns_none() {
        let props = make_props(&[
            ("ProviderName", string_val("Google")),
            ("PresentationIdentity", string_val("user@gmail.com")),
        ]);
        assert!(parse_account(&props).is_none());
    }

    #[test]
    fn parse_account_defaults_for_missing_fields() {
        let props = make_props(&[("Id", string_val("acc_minimal"))]);
        let (id, account) = parse_account(&props).unwrap();
        assert_eq!(id, "acc_minimal");
        assert_eq!(account.provider_name, "Unknown");
        assert_eq!(account.presentation_identity, "");
        assert_eq!(account.status, AccountStatus::Active); // default: AttentionNeeded=false
        assert!(!account.locked); // default: IsLocked=false
        assert!(account.services.is_empty());
    }
}
