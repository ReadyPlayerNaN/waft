//! Public GOA D-Bus contracts and conservative capability discovery.
use anyhow::{Context, Result};
use futures_util::{StreamExt, stream};
use std::collections::{HashMap, HashSet};
use waft_protocol::entity::accounts::{
    AccountStatus, OnlineAccount, OnlineAccountProvider, ServiceInfo,
};
use zbus::zvariant::OwnedValue;
use zbus::{Connection, Proxy};

pub const GOA_BUS_NAME: &str = "org.gnome.OnlineAccounts";
pub const GOA_OBJECT_PATH: &str = "/org/gnome/OnlineAccounts";
pub const GOA_MANAGER_PATH: &str = "/org/gnome/OnlineAccounts/Manager";
pub const GOA_MANAGER_IFACE: &str = "org.gnome.OnlineAccounts.Manager";
pub const GOA_ACCOUNT_IFACE: &str = "org.gnome.OnlineAccounts.Account";
pub const IFACE_OBJECT_MANAGER: &str = "org.freedesktop.DBus.ObjectManager";
pub const IFACE_PROPERTIES: &str = "org.freedesktop.DBus.Properties";
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
pub type ManagedObjects =
    HashMap<zbus::zvariant::OwnedObjectPath, HashMap<String, HashMap<String, OwnedValue>>>;
pub type AccountSnapshot = Vec<(String, String, OnlineAccount)>;
pub type CapabilityHistory = HashMap<String, (String, HashSet<String>)>;
#[derive(Debug)]
pub struct UnsupportedAccountApi;
impl std::fmt::Display for UnsupportedAccountApi {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("required account properties unavailable")
    }
}
impl std::error::Error for UnsupportedAccountApi {}

fn string(props: &HashMap<String, OwnedValue>, name: &str) -> Option<String> {
    props
        .get(name)
        .and_then(|v| String::try_from(v.clone()).ok())
}
pub fn parse_account_status(attention: bool) -> AccountStatus {
    if attention {
        AccountStatus::NeedsAttention
    } else {
        AccountStatus::Active
    }
}
pub fn service_name_to_property(name: &str) -> Option<String> {
    KNOWN_SERVICES
        .iter()
        .find(|(_, id)| *id == name)
        .map(|(cap, _)| format!("{cap}Disabled"))
}
fn baseline(provider: &str) -> &'static [&'static str] {
    match provider {
        "google" | "exchange" => &["mail", "calendar", "contacts"],
        "ms_graph" => &["mail", "calendar", "contacts", "files"],
        "owncloud" | "webdav" => &["calendar", "contacts", "files"],
        "imap_smtp" => &["mail"],
        "kerberos" | "fedora" => &["ticketing"],
        _ => &[],
    }
}
fn parse_with_capabilities(
    props: &HashMap<String, OwnedValue>,
    capabilities: &HashSet<String>,
) -> Option<(String, OnlineAccount)> {
    let id = string(props, "Id")?;
    let provider_type = string(props, "ProviderType").unwrap_or_default();
    let supported = |id: &str| baseline(&provider_type).contains(&id) || capabilities.contains(id);
    let services = KNOWN_SERVICES
        .iter()
        .filter_map(|(cap, id)| {
            if !supported(id) {
                return None;
            }
            let disabled = bool::try_from(props.get(&format!("{cap}Disabled"))?.clone()).ok()?;
            Some(ServiceInfo {
                name: (*id).into(),
                enabled: !disabled,
            })
        })
        .collect();
    Some((
        id.clone(),
        OnlineAccount {
            id,
            provider_type,
            provider_name: string(props, "ProviderName").unwrap_or_else(|| "Unknown".into()),
            presentation_identity: string(props, "PresentationIdentity").unwrap_or_default(),
            status: parse_account_status(
                props
                    .get("AttentionNeeded")
                    .and_then(|v| bool::try_from(v.clone()).ok())
                    .unwrap_or(false),
            ),
            locked: props
                .get("IsLocked")
                .and_then(|v| bool::try_from(v.clone()).ok())
                .unwrap_or(false),
            services,
        },
    ))
}
pub fn parse_account(props: &HashMap<String, OwnedValue>) -> Option<(String, OnlineAccount)> {
    parse_with_capabilities(props, &HashSet::new())
}

/// A malformed account makes discovery fail, never a successful partial snapshot.
pub fn parse_snapshot(
    objects: ManagedObjects,
    history: &mut CapabilityHistory,
) -> Result<AccountSnapshot> {
    let mut accounts = Vec::new();
    let mut seen = HashSet::new();
    for (path, interfaces) in objects {
        let Some(props) = interfaces.get(GOA_ACCOUNT_IFACE) else {
            continue;
        };
        let id = string(props, "Id").context("GOA account missing Id")?;
        anyhow::ensure!(!id.is_empty(), "empty GOA account Id");
        anyhow::ensure!(seen.insert(id.clone()), "duplicate GOA account Id");
        let provider = string(props, "ProviderType")
            .filter(|s| !s.is_empty())
            .ok_or(UnsupportedAccountApi)?;
        for required in ["IsLocked", "AttentionNeeded"] {
            let property = props.get(required).ok_or(UnsupportedAccountApi)?;
            bool::try_from(property.clone()).context("invalid required account boolean")?;
        }
        let entry = history
            .entry(id.clone())
            .or_insert_with(|| (provider.clone(), HashSet::new()));
        if entry.0 != provider {
            *entry = (provider, HashSet::new());
        }
        for (cap, service) in KNOWN_SERVICES {
            if interfaces.contains_key(&format!("org.gnome.OnlineAccounts.{cap}")) {
                entry.1.insert((*service).into());
            }
        }
        let (_, account) =
            parse_with_capabilities(props, &entry.1).context("invalid GOA account")?;
        accounts.push((id, path.to_string(), account));
    }
    history.retain(|id, _| seen.contains(id));
    accounts.sort_by(|a, b| a.0.cmp(&b.0));
    Ok(accounts)
}

pub fn snapshot_identities(objects: &ManagedObjects) -> HashMap<String, String> {
    objects
        .values()
        .filter_map(|interfaces| {
            let props = interfaces.get(GOA_ACCOUNT_IFACE)?;
            Some((
                string(props, "Id")?,
                string(props, "Identity").unwrap_or_default(),
            ))
        })
        .collect()
}

pub async fn managed_objects(conn: &Connection, owner: &str) -> Result<ManagedObjects> {
    let proxy = Proxy::new(conn, owner, GOA_OBJECT_PATH, IFACE_OBJECT_MANAGER).await?;
    Ok(proxy.call("GetManagedObjects", &()).await?)
}
pub async fn set_service_disabled(
    conn: &Connection,
    owner: &str,
    path: &str,
    name: &str,
    disabled: bool,
) -> Result<()> {
    let property = service_name_to_property(name).context("unsupported service")?;
    let proxy = Proxy::new(conn, owner, path, IFACE_PROPERTIES).await?;
    proxy
        .call::<_, _, ()>(
            "Set",
            &(
                GOA_ACCOUNT_IFACE,
                property,
                zbus::zvariant::Value::from(disabled),
            ),
        )
        .await?;
    Ok(())
}
pub async fn remove_account(conn: &Connection, owner: &str, path: &str) -> Result<()> {
    Proxy::new(conn, owner, path, GOA_ACCOUNT_IFACE)
        .await?
        .call::<_, _, ()>("Remove", &())
        .await?;
    Ok(())
}
const PROVIDERS: &[(&str, &str, &str)] = &[
    ("google", "Google", "goa-account-google"),
    ("ms_graph", "Microsoft 365", "goa-account-ms365"),
    ("ms365", "Microsoft 365", "goa-account-ms365"),
    ("owncloud", "Nextcloud", "goa-account-owncloud"),
    ("imap_smtp", "IMAP and SMTP", "goa-account-imap-smtp"),
    ("exchange", "Microsoft Exchange", "goa-account-exchange"),
    ("kerberos", "Kerberos", "goa-account-kerberos"),
    ("fedora", "Fedora", "goa-account-fedora"),
    ("webdav", "WebDAV", "x-office-calendar-symbolic"),
];
/// Commit only a complete probe set: errors must not masquerade as unsupported.
pub async fn discover_providers(
    conn: &Connection,
    owner: &str,
    accounts: &AccountSnapshot,
) -> Result<Vec<OnlineAccountProvider>> {
    let mut candidates: HashMap<String, (String, Option<String>)> = PROVIDERS
        .iter()
        .map(|(id, name, icon)| ((*id).into(), ((*name).into(), Some((*icon).into()))))
        .collect();
    for (_, _, account) in accounts {
        if !account.provider_type.is_empty() {
            candidates
                .entry(account.provider_type.clone())
                .or_insert((account.provider_name.clone(), None));
        }
    }
    let results = stream::iter(candidates.into_iter().map(|(id, (name, icon))| async move {
        let probe = async {
            let proxy = Proxy::new(conn, owner, GOA_MANAGER_PATH, GOA_MANAGER_IFACE).await?;
            let supported: bool = proxy.call("IsSupportedProvider", &(id.as_str(),)).await?;
            Ok::<_, anyhow::Error>(supported.then_some(OnlineAccountProvider {
                provider_type: id,
                provider_name: name,
                icon_name: icon,
            }))
        };
        tokio::time::timeout(std::time::Duration::from_secs(2), probe)
            .await
            .context("provider probe timeout")?
    }))
    .buffer_unordered(4)
    .collect::<Vec<_>>()
    .await;
    let mut providers = Vec::new();
    for result in results {
        if let Some(provider) = result? {
            providers.push(provider);
        }
    }
    providers.sort_by(|a, b| a.provider_type.cmp(&b.provider_type));
    Ok(providers)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn value(s: &str) -> OwnedValue {
        zbus::zvariant::Value::from(s.to_string())
            .try_into()
            .expect("string")
    }
    fn props(provider: &str) -> HashMap<String, OwnedValue> {
        let mut props = HashMap::from([
            ("Id".into(), value("fixture")),
            ("ProviderType".into(), value(provider)),
            ("IsLocked".into(), OwnedValue::from(false)),
            ("AttentionNeeded".into(), OwnedValue::from(false)),
        ]);
        for (cap, _) in KNOWN_SERVICES {
            props.insert(format!("{cap}Disabled"), OwnedValue::from(false));
        }
        props
    }
    #[test]
    fn pinned_base_property_fixture_does_not_enumerate_capabilities() {
        let xml = roxmltree::Document::parse(include_str!("../tests/fixtures/contracts.xml"))
            .expect("pinned XML");
        let mut fields = props("imap_smtp");
        for property in xml.descendants().filter(|n| n.has_tag_name("property")) {
            let name = property.attribute("name").expect("property name");
            if name.ends_with("Disabled") {
                assert_eq!(property.attribute("type"), Some("b"));
                fields.insert(name.into(), OwnedValue::from(false));
            }
        }
        assert_eq!(parse_account(&fields).expect("account").1.services.len(), 1);
        let probe = xml
            .descendants()
            .find(|n| n.attribute("name") == Some("IsSupportedProvider"))
            .expect("probe");
        let args: Vec<_> = probe
            .children()
            .filter(|n| n.has_tag_name("arg"))
            .map(|n| (n.attribute("direction"), n.attribute("type")))
            .collect();
        assert_eq!(args, [(Some("in"), Some("s")), (Some("out"), Some("b"))]);
    }
    #[test]
    fn missing_lock_metadata_is_not_mutable_healthy_state() {
        let mut properties = props("google");
        properties.remove("IsLocked");
        let path = zbus::zvariant::OwnedObjectPath::try_from("/fixture").expect("path");
        let error = parse_snapshot(
            HashMap::from([(
                path,
                HashMap::from([(GOA_ACCOUNT_IFACE.into(), properties)]),
            )]),
            &mut CapabilityHistory::new(),
        )
        .expect_err("required API");
        assert!(error.downcast_ref::<UnsupportedAccountApi>().is_some());
    }
    #[test]
    fn base_disabled_properties_do_not_imply_provider_support() {
        let (_, account) = parse_account(&props("imap_smtp")).expect("account");
        assert_eq!(account.services.len(), 1);
        assert_eq!(account.services[0].name, "mail");
    }
    #[test]
    fn disabled_known_service_remains_available() {
        let mut props = props("google");
        props.insert("CalendarDisabled".into(), OwnedValue::from(true));
        let (_, account) = parse_account(&props).expect("account");
        assert_eq!(account.services.len(), 3);
        assert!(
            !account
                .services
                .iter()
                .find(|s| s.name == "calendar")
                .expect("calendar")
                .enabled
        );
        assert!(!account.services.iter().any(|s| s.name == "files"));
    }
    #[test]
    fn unknown_provider_has_no_invented_services() {
        assert!(
            parse_account(&props("custom"))
                .expect("account")
                .1
                .services
                .is_empty()
        );
    }
    #[test]
    fn observed_capability_survives_disable_but_not_deletion() {
        let path =
            zbus::zvariant::OwnedObjectPath::try_from("/org/gnome/OnlineAccounts/Accounts/fixture")
                .expect("path");
        let interfaces = HashMap::from([
            (GOA_ACCOUNT_IFACE.into(), props("custom")),
            ("org.gnome.OnlineAccounts.Calendar".into(), HashMap::new()),
        ]);
        let mut history = CapabilityHistory::new();
        assert_eq!(
            parse_snapshot(HashMap::from([(path.clone(), interfaces)]), &mut history)
                .expect("snapshot")[0]
                .2
                .services
                .len(),
            1
        );
        let interfaces = HashMap::from([(GOA_ACCOUNT_IFACE.into(), props("custom"))]);
        assert_eq!(
            parse_snapshot(HashMap::from([(path, interfaces)]), &mut history).expect("snapshot")[0]
                .2
                .services
                .len(),
            1
        );
        parse_snapshot(HashMap::new(), &mut history).expect("empty snapshot");
        assert!(history.is_empty());
    }
    #[test]
    fn malformed_snapshot_is_not_successful_empty_state() {
        let path = zbus::zvariant::OwnedObjectPath::try_from("/fixture").expect("path");
        assert!(
            parse_snapshot(
                HashMap::from([(
                    path,
                    HashMap::from([(GOA_ACCOUNT_IFACE.into(), HashMap::new())])
                )]),
                &mut CapabilityHistory::new()
            )
            .is_err()
        );
    }
    #[test]
    fn attention_is_not_a_credential_diagnosis() {
        assert_eq!(parse_account_status(true), AccountStatus::NeedsAttention);
        assert_eq!(parse_account_status(false), AccountStatus::Active);
    }
    #[test]
    fn property_mapping_covers_all_services() {
        for (cap, id) in KNOWN_SERVICES {
            assert_eq!(service_name_to_property(id), Some(format!("{cap}Disabled")));
        }
        assert!(service_name_to_property("unknown").is_none());
    }
}
