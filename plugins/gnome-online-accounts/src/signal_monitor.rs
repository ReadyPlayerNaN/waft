//! Subscription-before-snapshot signal invalidation, with sender isolation.
use crate::{dbus, state::GoaState};
use anyhow::Result;
use futures_util::{StreamExt, stream::SelectAll};
use std::sync::{Arc, Mutex};
use waft_plugin::{EntityNotifier, StateLocker};
use zbus::{Connection, MatchRule, MessageStream};

/// Owned streams unregister their match rules on drop.
pub async fn subscribe(conn: &Connection) -> Result<Vec<MessageStream>> {
    let owner = MatchRule::builder()
        .msg_type(zbus::message::Type::Signal)
        .sender("org.freedesktop.DBus")?
        .interface("org.freedesktop.DBus")?
        .member("NameOwnerChanged")?
        .add_arg(dbus::GOA_BUS_NAME)?
        .build();
    let accounts = MatchRule::builder()
        .msg_type(zbus::message::Type::Signal)
        .sender(dbus::GOA_BUS_NAME)?
        .path_namespace(dbus::GOA_OBJECT_PATH)?
        .interface(dbus::IFACE_OBJECT_MANAGER)?
        .build();
    let properties = MatchRule::builder()
        .msg_type(zbus::message::Type::Signal)
        .sender(dbus::GOA_BUS_NAME)?
        .path_namespace(dbus::GOA_OBJECT_PATH)?
        .interface(dbus::IFACE_PROPERTIES)?
        .member("PropertiesChanged")?
        .add_arg(dbus::GOA_ACCOUNT_IFACE)?
        .build();
    Ok(vec![
        MessageStream::for_match_rule(owner, conn, Some(256)).await?,
        MessageStream::for_match_rule(accounts, conn, Some(256)).await?,
        MessageStream::for_match_rule(properties, conn, Some(256)).await?,
    ])
}

pub async fn read_signals(
    streams: Vec<MessageStream>,
    state: Arc<Mutex<GoaState>>,
    wake: Arc<tokio::sync::Notify>,
    changed: Arc<tokio::sync::Notify>,
    notifier: EntityNotifier,
) {
    let mut streams: SelectAll<_> = streams.into_iter().collect();
    while let Some(result) = streams.next().await {
        let Ok(message) = result else {
            break;
        };
        if invalidate(&message, &state) {
            changed.notify_waiters();
            notifier.notify();
            wake.notify_one();
        }
    }
    {
        let mut st = state.lock_or_recover();
        st.announced_owner = None;
        st.dependency_lost();
    }
    changed.notify_waiters();
    log::warn!("[goa] signal reader stopped; dependency state will be recovered");
    notifier.notify();
    wake.notify_one();
}

fn invalidate(message: &zbus::Message, state: &Arc<Mutex<GoaState>>) -> bool {
    let header = message.header();
    if header.message_type() != zbus::message::Type::Signal {
        return false;
    }
    let sender = header.sender().map(zbus::names::UniqueName::as_str);
    let iface = header.interface().map(zbus::names::InterfaceName::as_str);
    let member = header.member().map(zbus::names::MemberName::as_str);
    let mut st = state.lock_or_recover();
    if sender == Some("org.freedesktop.DBus")
        && iface == Some("org.freedesktop.DBus")
        && member == Some("NameOwnerChanged")
    {
        let Ok((name, _old, new)) = message.body().deserialize::<(String, String, String)>() else {
            return false;
        };
        if name != dbus::GOA_BUS_NAME {
            return false;
        }
        if st.session.as_ref().is_some_and(|s| s.owner == new) {
            return false;
        }
        st.announced_owner = (!new.is_empty()).then_some(new);
        st.dependency_lost();
        return true;
    }
    if !st
        .session
        .as_ref()
        .is_some_and(|s| sender == Some(s.owner.as_str()))
    {
        return false;
    }
    let path = header
        .path()
        .map(zbus::zvariant::ObjectPath::as_str)
        .unwrap_or("");
    if !path.starts_with(&format!("{}/", dbus::GOA_OBJECT_PATH)) && path != dbus::GOA_OBJECT_PATH {
        return false;
    }
    if iface == Some(dbus::IFACE_PROPERTIES) && member == Some("PropertiesChanged") {
        // Identity/property replacement is an action fence, even before its snapshot commits.
        if let Some(id) = st.id_for_path(path).map(str::to_owned)
            && let Ok((_, props, invalidated)) = message.body().deserialize::<(
                String,
                std::collections::HashMap<String, zbus::zvariant::OwnedValue>,
                Vec<String>,
            )>()
        {
            let identity_changed = props
                .get("Identity")
                .and_then(|v| String::try_from(v.clone()).ok())
                .is_some_and(|value| st.identities.get(&id) != Some(&value));
            let provider_changed = props
                .get("ProviderType")
                .and_then(|v| String::try_from(v.clone()).ok())
                .is_some_and(|value| {
                    st.accounts
                        .get(&id)
                        .is_some_and(|a| a.provider_type != value)
                });
            if identity_changed
                || provider_changed
                || invalidated
                    .iter()
                    .any(|p| matches!(p.as_str(), "Identity" | "ProviderType"))
            {
                *st.incarnations.entry(id.clone()).or_default() += 1;
                *st.replacements.entry(id).or_default() += 1;
            }
        }
        // Invalidated properties and malformed relevant bodies require a full reread too.
        st.revision += 1;
        return true;
    }
    if iface == Some(dbus::IFACE_OBJECT_MANAGER)
        && matches!(member, Some("InterfacesAdded" | "InterfacesRemoved"))
    {
        let object_path = message
            .body()
            .deserialize::<(zbus::zvariant::OwnedObjectPath, Vec<String>)>()
            .ok()
            .filter(|(_, interfaces)| interfaces.iter().any(|i| i == dbus::GOA_ACCOUNT_IFACE))
            .map(|(p, _)| p.to_string());
        if let Some(path) = object_path
            && let Some(id) = st.id_for_path(&path).map(str::to_owned)
        {
            *st.incarnations.entry(id).or_default() += 1;
        }
        // Added account properties can reveal a reused identity before a snapshot.
        if member == Some("InterfacesAdded")
            && let Ok((_, interfaces)) = message.body().deserialize::<(
                zbus::zvariant::OwnedObjectPath,
                std::collections::HashMap<
                    String,
                    std::collections::HashMap<String, zbus::zvariant::OwnedValue>,
                >,
            )>()
            && let Some(props) = interfaces.get(dbus::GOA_ACCOUNT_IFACE)
            && let Some((id, _)) = dbus::parse_account(props)
        {
            *st.incarnations.entry(id.clone()).or_default() += 1;
            *st.replacements.entry(id).or_default() += 1;
        }
        st.revision += 1;
        return true;
    }
    false
}
