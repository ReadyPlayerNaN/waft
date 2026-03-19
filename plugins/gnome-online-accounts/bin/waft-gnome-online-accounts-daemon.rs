//! GNOME Online Accounts daemon -- monitors GOA via D-Bus and exposes
//! `online-account` entities with per-service toggles and
//! `online-account-provider` entities for available account providers.
//!
//! Entity types:
//! - `online-account` with actions: `enable-service`, `disable-service`, `remove-account`
//! - `online-account-provider` with actions: `add-account`

use std::sync::LazyLock;

use anyhow::{Context, Result};
use log::{debug, error, info, warn};
use std::sync::{Arc, Mutex as StdMutex};
use waft_plugin::*;
use waft_plugin_gnome_online_accounts::dbus;
use waft_plugin_gnome_online_accounts::signal_monitor::monitor_goa_signals;
use waft_plugin_gnome_online_accounts::state::GoaState;
use waft_protocol::entity::accounts::{
    ONLINE_ACCOUNT_ENTITY_TYPE, ONLINE_ACCOUNT_PROVIDER_ENTITY_TYPE,
};
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

    /// Handle actions on `online-account-provider` entities.
    async fn handle_provider_action(
        &self,
        urn: Urn,
        action: String,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let provider_type = urn.id().to_string();

        match action.as_str() {
            "add-account" => {
                info!("[goa] Add account requested for provider: {}", provider_type);

                // Spawn the add-account helper as a subprocess.
                // Use our own binary with --add-account flag.
                let self_binary = std::env::current_exe()
                    .map_err(|e| -> Box<dyn std::error::Error + Send + Sync> {
                        format!("failed to get current exe: {e}").into()
                    })?;

                let child = tokio::process::Command::new(&self_binary)
                    .arg("--add-account")
                    .arg(&provider_type)
                    .stdin(std::process::Stdio::null())
                    .stdout(std::process::Stdio::null())
                    .stderr(std::process::Stdio::inherit())
                    .spawn();

                match child {
                    Ok(mut child) => {
                        // Don't block the action handler waiting for the dialog.
                        // Spawn a task to reap the child process.
                        tokio::spawn(async move {
                            match child.wait().await {
                                Ok(status) => {
                                    debug!("[goa] add-account helper exited: {}", status);
                                }
                                Err(e) => {
                                    warn!("[goa] add-account helper wait error: {}", e);
                                }
                            }
                        });
                    }
                    Err(e) => {
                        error!("[goa] Failed to spawn add-account helper: {}", e);
                        return Err(format!("failed to spawn add-account helper: {e}").into());
                    }
                }
            }
            _ => {
                debug!("[goa] Unknown provider action: {}", action);
            }
        }

        Ok(())
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
    ) -> Result<serde_json::Value, Box<dyn std::error::Error + Send + Sync>> {
        // Handle provider-level actions
        if urn.entity_type() == ONLINE_ACCOUNT_PROVIDER_ENTITY_TYPE {
            self.handle_provider_action(urn, action).await?;
            return Ok(serde_json::Value::Null);
        }

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

        Ok(serde_json::Value::Null)
    }
}

/// Handle `--add-account <provider-type>` invocation.
///
/// This runs in a separate process spawned by the daemon's `add-account` action.
/// It opens GNOME Settings to the online accounts page to trigger the native
/// add-account flow. The daemon's existing D-Bus signal monitor detects the
/// new account via `InterfacesAdded` automatically.
fn run_add_account(provider_type: &str) -> Result<()> {
    info!(
        "[goa] add-account helper invoked for provider: {}",
        provider_type
    );

    // Use gnome-control-center to trigger the native add-account flow.
    // GOA's own dialog handles OAuth, WebKit, form-based flows etc.
    let status = std::process::Command::new("gnome-control-center")
        .arg("online-accounts")
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::inherit())
        .status();

    match status {
        Ok(s) => {
            if s.success() {
                info!("[goa] add-account helper completed successfully");
            } else {
                warn!("[goa] gnome-control-center exited with: {}", s);
            }
        }
        Err(e) => {
            error!(
                "[goa] Failed to launch gnome-control-center: {}. \
                 Install gnome-control-center or GNOME Settings for add-account support.",
                e
            );
        }
    }

    Ok(())
}

fn main() -> Result<()> {
    // Check for --add-account flag before manifest handling.
    let args: Vec<String> = std::env::args().collect();
    if let Some(pos) = args.iter().position(|a| a == "--add-account") {
        waft_plugin::init_plugin_logger("info");
        let provider_type = args
            .get(pos + 1)
            .context("--add-account requires a provider type argument")?;
        return run_add_account(provider_type);
    }

    if waft_plugin::manifest::handle_provides_i18n(
        &[ONLINE_ACCOUNT_ENTITY_TYPE, ONLINE_ACCOUNT_PROVIDER_ENTITY_TYPE],
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

        // Discover available providers
        let providers = dbus::discover_providers(&conn).await;
        if providers.is_empty() {
            info!("[goa] No supported providers found (goa-daemon may not be running)");
        } else {
            info!("[goa] Found {} supported provider(s)", providers.len());
            let mut st = state.lock_or_recover();
            st.providers = providers;
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
