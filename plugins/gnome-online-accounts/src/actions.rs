//! Validated, non-reentrant account actions against the current GOA generation.
use crate::{
    account_settings_launch::SettingsTarget,
    dbus,
    lifecycle::{GoaLifecycle, failure},
};
use std::time::Duration;
use waft_plugin::{Entity, Plugin, StateLocker, Urn};
use waft_protocol::entity::accounts::{
    Availability, ONLINE_ACCOUNT_ENTITY_TYPE, ONLINE_ACCOUNT_PROVIDER_ENTITY_TYPE,
    ONLINE_ACCOUNTS_STATUS_ENTITY_TYPE,
};

pub struct GoaPlugin {
    pub lifecycle: GoaLifecycle,
}
#[async_trait::async_trait]
impl Plugin for GoaPlugin {
    fn get_entities(&self) -> Vec<Entity> {
        self.lifecycle.state.lock_or_recover().get_entities()
    }
    fn can_stop(&self) -> bool {
        let st = self.lifecycle.state.lock_or_recover();
        st.busy.is_empty() && st.launch_targets.is_empty()
    }
    async fn handle_action(
        &self,
        urn: Urn,
        action: String,
        params: serde_json::Value,
    ) -> anyhow::Result<serde_json::Value> {
        if urn != Urn::new("gnome-online-accounts", urn.entity_type(), urn.id()) {
            return Err(failure(
                "protocol.validation",
                "Unsupported account action target",
            ));
        }
        if urn.entity_type() == ONLINE_ACCOUNTS_STATUS_ENTITY_TYPE
            && urn.id() == "singleton"
            && action == "open-account-settings"
        {
            return self
                .lifecycle
                .launch_settings(SettingsTarget::Generic)
                .await;
        }
        if urn.entity_type() == ONLINE_ACCOUNT_PROVIDER_ENTITY_TYPE && action == "add-account" {
            {
                let st = self.lifecycle.state.lock_or_recover();
                if st.status.providers != Availability::Ready {
                    return Err(failure(
                        "goa.unavailable",
                        "Provider discovery is not ready",
                    ));
                }
                if !st.providers.iter().any(|p| p.provider_type == urn.id()) {
                    return Err(failure("entity.not-found", "Provider is unavailable"));
                }
            }
            return self
                .lifecycle
                .launch_settings(SettingsTarget::Provider(urn.id().into()))
                .await;
        }
        if urn.entity_type() != ONLINE_ACCOUNT_ENTITY_TYPE {
            return Err(failure(
                "protocol.validation",
                "Unsupported account action target",
            ));
        }
        let id = urn.id().to_string();
        if action == "open-account-settings" {
            if !self
                .lifecycle
                .state
                .lock_or_recover()
                .accounts
                .contains_key(&id)
            {
                return Err(failure("entity.not-found", "Account no longer exists"));
            }
            return self
                .lifecycle
                .launch_settings(SettingsTarget::Account(id))
                .await;
        }
        if !matches!(
            action.as_str(),
            "enable-service" | "disable-service" | "remove-account"
        ) {
            return Err(failure("protocol.validation", "Unknown account action"));
        }
        let entered = tokio::time::Instant::now();
        let (session, incarnation, replacement) = {
            let mut st = self.lifecycle.state.lock_or_recover();
            if st.status.accounts != Availability::Ready {
                return Err(failure("goa.unavailable", "Account discovery is not ready"));
            }
            let session = st
                .session
                .clone()
                .ok_or_else(|| failure("goa.unavailable", "Account connection is unavailable"))?;
            if !st.accounts.contains_key(&id) {
                return Err(failure("entity.not-found", "Account no longer exists"));
            }
            if !st.busy.insert(id.clone()) {
                return Err(failure(
                    "goa.busy",
                    "An account action is already in progress",
                ));
            }
            (
                session,
                st.incarnations.get(&id).copied().unwrap_or(0),
                st.replacements.get(&id).copied().unwrap_or(0),
            )
        };
        let _permit = ActionPermit {
            lifecycle: self.lifecycle.clone(),
            id: id.clone(),
        };
        let dispatched = std::sync::atomic::AtomicBool::new(false);
        let operation = async {
            let accounts = self
                .lifecycle
                .coherent_snapshot(&session)
                .await
                .map_err(|_| {
                    failure(
                        "goa.outcome-unconfirmed",
                        "Account state could not be confirmed",
                    )
                })?;
            let (_, path, account) = accounts
                .iter()
                .find(|(account_id, _, _)| account_id == &id)
                .ok_or_else(|| failure("entity.not-found", "Account no longer exists"))?;
            if account.locked {
                return Err(failure("goa.locked", "Account is administrator-locked"));
            }
            {
                let mut st = self.lifecycle.state.lock_or_recover();
                if !st
                    .session
                    .as_ref()
                    .is_some_and(|s| s.generation == session.generation)
                    || st.incarnations.get(&id).copied().unwrap_or(0) != incarnation
                {
                    return Err(failure(
                        "goa.outcome-unconfirmed",
                        "Account changed before dispatch",
                    ));
                }
                // Dirty before sending: cancellation cannot leave a pre-write snapshot authoritative.
                st.revision += 1;
            }
            self.lifecycle.wake.notify_one();
            let service = if action != "remove-account" {
                let service = params
                    .get("service_name")
                    .and_then(serde_json::Value::as_str)
                    .ok_or_else(|| {
                        failure("protocol.validation", "Missing service_name parameter")
                    })?;
                if !account.services.iter().any(|s| s.name == service) {
                    return Err(failure(
                        "goa.service-unsupported",
                        "Service capability is not established",
                    ));
                }
                Some(service)
            } else {
                None
            };
            dispatched.store(true, std::sync::atomic::Ordering::SeqCst);
            if let Some(service) = service {
                dbus::set_service_disabled(
                    &session.conn,
                    &session.owner,
                    path,
                    service,
                    action == "disable-service",
                )
                .await
                .map_err(|_| {
                    failure(
                        "goa.outcome-unconfirmed",
                        "Service change could not be confirmed",
                    )
                })?;
            } else {
                dbus::remove_account(&session.conn, &session.owner, path)
                    .await
                    .map_err(|_| {
                        failure(
                            "goa.outcome-unconfirmed",
                            "Account removal could not be confirmed",
                        )
                    })?;
            }
            let confirmed = self
                .lifecycle
                .coherent_snapshot(&session)
                .await
                .map_err(|_| {
                    failure(
                        "goa.outcome-unconfirmed",
                        "Action outcome could not be confirmed",
                    )
                })?;
            let current = confirmed
                .iter()
                .find(|(account_id, _, _)| account_id == &id);
            {
                let st = self.lifecycle.state.lock_or_recover();
                if !st
                    .session
                    .as_ref()
                    .is_some_and(|s| s.generation == session.generation)
                    || st.replacements.get(&id).copied().unwrap_or(0) != replacement
                {
                    return Err(failure(
                        "goa.outcome-unconfirmed",
                        "Account replaced during the action",
                    ));
                }
            }
            if let Some(service) = service {
                let st = self.lifecycle.state.lock_or_recover();
                if st.incarnations.get(&id).copied().unwrap_or(0) != incarnation {
                    return Err(failure(
                        "goa.outcome-unconfirmed",
                        "Account changed during the action",
                    ));
                }
                if !current.is_some_and(|(_, _, a)| {
                    a.services
                        .iter()
                        .any(|s| s.name == service && s.enabled == (action == "enable-service"))
                }) {
                    return Err(failure(
                        "goa.outcome-unconfirmed",
                        "Service change was not applied",
                    ));
                }
                Ok(serde_json::json!({"outcome":"applied"}))
            } else if current.is_none() {
                Ok(serde_json::json!({"outcome":"removed"}))
            } else {
                Err(failure(
                    "goa.outcome-unconfirmed",
                    "Account removal was not applied",
                ))
            }
        };
        let cancelled = async {
            loop {
                let changed = self.lifecycle.changed.notified();
                tokio::pin!(changed);
                changed.as_mut().enable();
                let valid = {
                    let st = self.lifecycle.state.lock_or_recover();
                    st.session
                        .as_ref()
                        .is_some_and(|s| s.generation == session.generation)
                        && (!dispatched.load(std::sync::atomic::Ordering::SeqCst)
                            || (st.replacements.get(&id).copied().unwrap_or(0) == replacement
                                && (action == "remove-account"
                                    || st.incarnations.get(&id).copied().unwrap_or(0)
                                        == incarnation)))
                };
                if !valid {
                    break;
                }
                changed.await;
            }
        };
        tokio::select! {
            biased;
            result = tokio::time::timeout_at(entered + Duration::from_secs(4), operation) => result.map_err(|_| failure("action.timeout", "Account action timed out; state will be reconciled"))?,
            _ = cancelled => Err(failure(if dispatched.load(std::sync::atomic::Ordering::SeqCst) { "goa.outcome-unconfirmed" } else { "goa.unavailable" }, "Account generation changed; current state must be reloaded")),
        }
    }
    async fn handle_action_cancelled(
        &self,
        _urn: &Urn,
        _action: &str,
        _params: &serde_json::Value,
    ) {
        self.lifecycle.invalidate();
    }
}
struct ActionPermit {
    lifecycle: GoaLifecycle,
    id: String,
}
impl Drop for ActionPermit {
    fn drop(&mut self) {
        self.lifecycle.state.lock_or_recover().busy.remove(&self.id);
        self.lifecycle.invalidate();
    }
}
