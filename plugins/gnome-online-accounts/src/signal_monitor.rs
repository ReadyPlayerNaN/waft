//! GOA D-Bus signal monitoring for account additions, removals, and property changes.

use std::collections::HashMap;
use std::sync::{Arc, Mutex as StdMutex};

use anyhow::{Context, Result};
use futures_util::StreamExt;
use log::{debug, info, warn};
use waft_plugin::{EntityNotifier, StateLocker};
use zbus::Connection;
use zbus::zvariant::OwnedValue;

use crate::dbus::{self, GOA_ACCOUNT_IFACE, GOA_BUS_NAME, IFACE_OBJECT_MANAGER, IFACE_PROPERTIES};
use crate::state::GoaState;

/// Monitor GOA D-Bus signals for live account updates.
///
/// Handles:
/// - `PropertiesChanged` on account objects (service toggles, attention state)
/// - `InterfacesAdded` on ObjectManager (new accounts)
/// - `InterfacesRemoved` on ObjectManager (removed accounts)
pub async fn monitor_goa_signals(
    conn: Connection,
    state: Arc<StdMutex<GoaState>>,
    notifier: EntityNotifier,
) -> Result<()> {
    let props_rule = zbus::MatchRule::builder()
        .msg_type(zbus::message::Type::Signal)
        .sender(GOA_BUS_NAME)?
        .interface(IFACE_PROPERTIES)?
        .member("PropertiesChanged")?
        .build();

    let obj_mgr_rule = zbus::MatchRule::builder()
        .msg_type(zbus::message::Type::Signal)
        .sender(GOA_BUS_NAME)?
        .interface(IFACE_OBJECT_MANAGER)?
        .build();

    let dbus_proxy = zbus::fdo::DBusProxy::new(&conn)
        .await
        .context("Failed to create DBus proxy")?;

    dbus_proxy
        .add_match_rule(props_rule)
        .await
        .context("Failed to add PropertiesChanged match rule")?;

    dbus_proxy
        .add_match_rule(obj_mgr_rule)
        .await
        .context("Failed to add ObjectManager match rule")?;

    info!("[goa] Listening for GOA PropertiesChanged and ObjectManager signals");

    let mut stream = zbus::MessageStream::from(&conn);
    while let Some(msg) = stream.next().await {
        let msg = match msg {
            Ok(m) => m,
            Err(e) => {
                warn!("[goa] D-Bus stream error: {e}");
                continue;
            }
        };

        let header = msg.header();
        let member = match header.member() {
            Some(m) => m.as_str().to_string(),
            None => continue,
        };
        let iface = match header.interface() {
            Some(i) => i.as_str().to_string(),
            None => continue,
        };
        let obj_path = match header.path() {
            Some(p) => p.to_string(),
            None => continue,
        };

        let mut changed = false;

        if iface == IFACE_PROPERTIES && member == "PropertiesChanged" {
            let Ok((prop_iface, props, _invalidated)) =
                msg.body()
                    .deserialize::<(String, HashMap<String, OwnedValue>, Vec<String>)>()
            else {
                continue;
            };

            if prop_iface == GOA_ACCOUNT_IFACE {
                changed = handle_account_properties_changed(&state, &obj_path, &props);
            }
        } else if iface == IFACE_OBJECT_MANAGER && member == "InterfacesAdded" {
            changed = handle_interfaces_added(&state, &msg);
        } else if iface == IFACE_OBJECT_MANAGER && member == "InterfacesRemoved" {
            changed = handle_interfaces_removed(&state, &msg);
        }

        if changed {
            notifier.notify();
        }
    }

    warn!("[goa] D-Bus signal stream ended -- signal monitoring is now unresponsive");

    Ok(())
}

/// Handle PropertiesChanged for a GOA account.
///
/// Updates the account in state if any relevant properties changed.
fn handle_account_properties_changed(
    state: &Arc<StdMutex<GoaState>>,
    obj_path: &str,
    props: &HashMap<String, OwnedValue>,
) -> bool {
    let mut st = state.lock_or_recover();

    let account_id = match st.id_for_path(obj_path) {
        Some(id) => id.to_string(),
        None => {
            debug!(
                "[goa] PropertiesChanged for unknown path {obj_path}, ignoring"
            );
            return false;
        }
    };

    let Some(account) = st.accounts.get_mut(&account_id) else {
        return false;
    };

    let mut changed = false;

    // Check AttentionNeeded
    if let Some(val) = props.get("AttentionNeeded")
        && let Ok(attention) = bool::try_from(val.clone())
    {
        let new_status = dbus::parse_account_status(attention);
        if account.status != new_status {
            info!(
                "[goa] Account {} status: {:?} -> {:?}",
                account_id, account.status, new_status
            );
            account.status = new_status;
            changed = true;
        }
    }

    // Check IsLocked
    if let Some(val) = props.get("IsLocked")
        && let Ok(locked) = bool::try_from(val.clone())
        && account.locked != locked
    {
        info!("[goa] Account {account_id} locked: {locked}");
        account.locked = locked;
        changed = true;
    }

    // Check service *Disabled properties
    for (capitalized, service_id) in dbus::KNOWN_SERVICES {
        let prop_name = format!("{capitalized}Disabled");
        if let Some(val) = props.get(&prop_name)
            && let Ok(disabled) = bool::try_from(val.clone())
        {
            let enabled = !disabled;
            if let Some(svc) = account.services.iter_mut().find(|s| s.name == *service_id)
                && svc.enabled != enabled
            {
                info!(
                    "[goa] Account {account_id} service {service_id} enabled: {enabled}"
                );
                svc.enabled = enabled;
                changed = true;
            }
        }
    }

    // Check PresentationIdentity
    if let Some(val) = props.get("PresentationIdentity")
        && let Ok(identity) = String::try_from(val.clone())
        && !identity.is_empty()
        && account.presentation_identity != identity
    {
        info!(
            "[goa] Account {} identity: {} -> {}",
            account_id, account.presentation_identity, identity
        );
        account.presentation_identity = identity;
        changed = true;
    }

    // Check ProviderName
    if let Some(val) = props.get("ProviderName")
        && let Ok(name) = String::try_from(val.clone())
        && !name.is_empty()
        && account.provider_name != name
    {
        info!(
            "[goa] Account {} provider: {} -> {}",
            account_id, account.provider_name, name
        );
        account.provider_name = name;
        changed = true;
    }

    changed
}

/// Handle InterfacesAdded: add new accounts when Account interface appears.
fn handle_interfaces_added(state: &Arc<StdMutex<GoaState>>, msg: &zbus::Message) -> bool {
    let Ok((path, interfaces)) = msg.body().deserialize::<(
        zbus::zvariant::OwnedObjectPath,
        HashMap<String, HashMap<String, OwnedValue>>,
    )>() else {
        return false;
    };

    let Some(account_props) = interfaces.get(GOA_ACCOUNT_IFACE) else {
        return false;
    };

    let path_str = path.to_string();

    let Some((id, account)) = dbus::parse_account(account_props) else {
        warn!(
            "[goa] InterfacesAdded for {path_str} but missing Id property"
        );
        return false;
    };

    let mut st = state.lock_or_recover();

    info!(
        "[goa] New account appeared: {} ({}) at {}",
        id, account.provider_name, path_str
    );
    st.update_account(id, path_str, account);

    true
}

/// Handle InterfacesRemoved: remove accounts when Account interface disappears.
fn handle_interfaces_removed(state: &Arc<StdMutex<GoaState>>, msg: &zbus::Message) -> bool {
    let Ok((path, interfaces)) = msg
        .body()
        .deserialize::<(zbus::zvariant::OwnedObjectPath, Vec<String>)>()
    else {
        return false;
    };

    if !interfaces.iter().any(|i| i == GOA_ACCOUNT_IFACE) {
        return false;
    }

    let path_str = path.to_string();
    let mut st = state.lock_or_recover();

    if let Some(id) = st.remove_by_path(&path_str) {
        info!("[goa] Account removed: {id} at {path_str}");
        true
    } else {
        debug!(
            "[goa] InterfacesRemoved for unknown path {path_str}, ignoring"
        );
        false
    }
}
