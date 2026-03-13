//! GNOME Online Accounts daemon -- monitors GOA via D-Bus and exposes
//! `online-account` entities with per-service toggles.
//!
//! Entity types:
//! - `online-account` with actions: `enable-service`, `disable-service`

use std::sync::LazyLock;

use anyhow::{Context, Result};
use log::{debug, error, info, warn};
use std::sync::{Arc, Mutex as StdMutex};
use waft_plugin::*;
use waft_plugin_gnome_online_accounts::dbus;
use waft_plugin_gnome_online_accounts::signal_monitor::monitor_goa_signals;
use waft_plugin_gnome_online_accounts::state::GoaState;
use waft_protocol::entity::accounts::ONLINE_ACCOUNT_ENTITY_TYPE;
use zbus::Connection;

static I18N: LazyLock<waft_i18n::I18n> = LazyLock::new(|| {
    waft_i18n::I18n::new(&[
        (
            "en-US",
            include_str!("../locales/en-US/gnome-online-accounts.ftl"),
        ),
        (
            "cs-CZ",
            include_str!("../locales/cs-CZ/gnome-online-accounts.ftl"),
        ),
    ])
});

fn i18n() -> &'static waft_i18n::I18n {
    &I18N
}

struct GoaPlugin {
    conn: Connection,
    state: Arc<StdMutex<GoaState>>,
}

impl GoaPlugin {
    fn lock_state(&self) -> std::sync::MutexGuard<'_, GoaState> {
        self.state.lock_or_recover()
    }
}

#[async_trait::async_trait]
impl Plugin for GoaPlugin {
    fn get_entities(&self) -> Vec<Entity> {
        let state = self.lock_state();
        state.get_entities()
    }

    async fn handle_action(
        &self,
        urn: Urn,
        action: String,
        params: serde_json::Value,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let account_id = urn.id().to_string();

        match action.as_str() {
            "enable-service" => {
                let service_name = match params["service_name"].as_str() {
                    Some(s) => s.to_string(),
                    None => {
                        warn!("[goa] enable-service action missing 'service_name' param");
                        return Err("missing service_name parameter".into());
                    }
                };

                debug!(
                    "[goa] Enable service '{}' on account {}",
                    service_name, account_id
                );

                let account_path = {
                    let state = self.lock_state();
                    match state.object_path_for_id(&account_id) {
                        Some(p) => p.to_string(),
                        None => {
                            warn!("[goa] Account not found: {}", account_id);
                            return Err(format!("account not found: {account_id}").into());
                        }
                    }
                };

                if let Err(e) =
                    dbus::set_service_disabled(&self.conn, &account_path, &service_name, false)
                        .await
                {
                    error!(
                        "[goa] Failed to enable service '{}' on {}: {}",
                        service_name, account_id, e
                    );
                    return Err(e.into());
                }

                // Optimistic update (signal monitoring will also catch this)
                {
                    let mut state = self.lock_state();
                    if let Some(account) = state.accounts.get_mut(&account_id)
                        && let Some(svc) =
                            account.services.iter_mut().find(|s| s.name == service_name)
                    {
                        svc.enabled = true;
                    }
                }
            }

            "disable-service" => {
                let service_name = match params["service_name"].as_str() {
                    Some(s) => s.to_string(),
                    None => {
                        warn!("[goa] disable-service action missing 'service_name' param");
                        return Err("missing service_name parameter".into());
                    }
                };

                debug!(
                    "[goa] Disable service '{}' on account {}",
                    service_name, account_id
                );

                let account_path = {
                    let state = self.lock_state();
                    match state.object_path_for_id(&account_id) {
                        Some(p) => p.to_string(),
                        None => {
                            warn!("[goa] Account not found: {}", account_id);
                            return Err(format!("account not found: {account_id}").into());
                        }
                    }
                };

                if let Err(e) =
                    dbus::set_service_disabled(&self.conn, &account_path, &service_name, true)
                        .await
                {
                    error!(
                        "[goa] Failed to disable service '{}' on {}: {}",
                        service_name, account_id, e
                    );
                    return Err(e.into());
                }

                // Optimistic update
                {
                    let mut state = self.lock_state();
                    if let Some(account) = state.accounts.get_mut(&account_id)
                        && let Some(svc) =
                            account.services.iter_mut().find(|s| s.name == service_name)
                    {
                        svc.enabled = false;
                    }
                }
            }

            "remove-account" => {
                let (account_path, locked) = {
                    let state = self.lock_state();
                    let path = match state.object_path_for_id(&account_id) {
                        Some(p) => p.to_string(),
                        None => {
                            warn!("[goa] Account not found: {}", account_id);
                            return Err(format!("account not found: {account_id}").into());
                        }
                    };
                    let locked = state
                        .accounts
                        .get(&account_id)
                        .map(|a| a.locked)
                        .unwrap_or(false);
                    (path, locked)
                };
                if locked {
                    return Err(format!("account {} is locked", account_id).into());
                }
                dbus::remove_account(&self.conn, &account_path)
                    .await
                    .map_err(|e| -> Box<dyn std::error::Error + Send + Sync> { e.into() })?;
            }

            _ => {
                debug!("[goa] Unknown action: {}", action);
            }
        }

        Ok(())
    }
}

fn main() -> Result<()> {
    if waft_plugin::manifest::handle_provides_i18n(
        &[ONLINE_ACCOUNT_ENTITY_TYPE],
        i18n(),
        "plugin-name",
        "plugin-description",
    ) {
        return Ok(());
    }

    waft_plugin::init_plugin_logger("info");

    info!("Starting GNOME Online Accounts plugin...");

    let rt = tokio::runtime::Runtime::new().context("failed to create tokio runtime")?;
    rt.block_on(async {
        let conn = match Connection::session().await {
            Ok(c) => c,
            Err(e) => {
                warn!("[goa] Failed to connect to session bus: {}", e);
                warn!("[goa] GOA plugin cannot function without session bus access, exiting");
                return Ok(());
            }
        };

        let state = Arc::new(StdMutex::new(GoaState::default()));

        // Discover initial accounts
        match dbus::discover_accounts(&conn).await {
            Ok(accounts) => {
                let mut st = state.lock_or_recover();
                for (id, path, account) in accounts {
                    info!(
                        "[goa] Account: {} ({}) at {}",
                        id, account.provider_name, path
                    );
                    st.update_account(id, path, account);
                }
                if st.accounts.is_empty() {
                    info!("[goa] No GOA accounts found");
                }
            }
            Err(e) => {
                warn!(
                    "[goa] Failed to discover accounts (goa-daemon may not be running): {}",
                    e
                );
            }
        }

        let plugin = GoaPlugin {
            conn: conn.clone(),
            state: state.clone(),
        };

        let (runtime, notifier) = PluginRuntime::new("gnome-online-accounts", plugin);

        // Monitor GOA D-Bus signals
        spawn_monitored_anyhow("goa/signal-monitor", async move {
            monitor_goa_signals(conn, state, notifier).await
        });

        runtime.run().await?;
        Ok(())
    })
}
