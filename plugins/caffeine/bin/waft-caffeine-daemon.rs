//! Caffeine daemon — screen lock/screensaver inhibition toggle.
//!
//! Provides a toggle to inhibit screen locking via xdg-desktop-portal (Inhibit)
//! or org.freedesktop.ScreenSaver as fallback.

use anyhow::{Context, Result, bail};
use log::{debug, info, warn};
use std::collections::HashMap;
use waft_plugin_sdk::*;
use zbus::Connection;
use zbus::zvariant::{OwnedObjectPath, Value};

const PORTAL_DESTINATION: &str = "org.freedesktop.portal.Desktop";
const PORTAL_PATH: &str = "/org/freedesktop/portal/desktop";
const PORTAL_INTERFACE: &str = "org.freedesktop.portal.Inhibit";

const SCREENSAVER_DESTINATION: &str = "org.freedesktop.ScreenSaver";
const SCREENSAVER_PATHS: &[&str] = &["/ScreenSaver", "/org/freedesktop/ScreenSaver"];
const SCREENSAVER_INTERFACE: &str = "org.freedesktop.ScreenSaver";

#[derive(Debug, Clone)]
enum Backend {
    Portal,
    ScreenSaver { path: &'static str },
}

/// Probe for available backend.
async fn probe_backend(conn: &Connection) -> Result<Backend> {
    // Try Portal first
    let portal_proxy = zbus::Proxy::new(
        conn,
        PORTAL_DESTINATION,
        PORTAL_PATH,
        "org.freedesktop.DBus.Peer",
    )
    .await?;
    if portal_proxy.call::<_, _, ()>("Ping", &()).await.is_ok() {
        info!("[caffeine] Portal backend available");
        return Ok(Backend::Portal);
    }

    // Try ScreenSaver
    for path in SCREENSAVER_PATHS {
        let proxy = zbus::Proxy::new(
            conn,
            SCREENSAVER_DESTINATION,
            *path,
            SCREENSAVER_INTERFACE,
        )
        .await?;
        if proxy.call::<_, _, (bool,)>("GetActive", &()).await.is_ok() {
            info!("[caffeine] ScreenSaver backend available at {}", path);
            return Ok(Backend::ScreenSaver { path });
        }
    }

    bail!("No screen inhibit backend available")
}

/// Mutable caffeine state behind interior mutability.
struct CaffeineState {
    active: bool,
    busy: bool,
    screensaver_cookie: Option<u32>,
}

struct CaffeineDaemon {
    conn: Connection,
    backend: Backend,
    state: std::sync::Mutex<CaffeineState>,
}

impl CaffeineDaemon {
    async fn new() -> Result<Self> {
        let conn = Connection::session()
            .await
            .context("Failed to connect to session bus")?;

        let backend = probe_backend(&conn).await?;
        info!("[caffeine] Using backend: {:?}", backend);

        Ok(Self {
            conn,
            backend,
            state: std::sync::Mutex::new(CaffeineState {
                active: false,
                busy: false,
                screensaver_cookie: None,
            }),
        })
    }

    fn lock_state(&self) -> std::sync::MutexGuard<'_, CaffeineState> {
        match self.state.lock() {
            Ok(g) => g,
            Err(e) => {
                warn!("[caffeine] mutex poisoned, recovering: {e}");
                e.into_inner()
            }
        }
    }

    async fn inhibit(&self) -> Result<()> {
        match &self.backend {
            Backend::Portal => {
                let proxy = zbus::Proxy::new(
                    &self.conn,
                    PORTAL_DESTINATION,
                    PORTAL_PATH,
                    PORTAL_INTERFACE,
                )
                .await?;

                let mut options: HashMap<&str, Value> = HashMap::new();
                options.insert("reason", Value::from("User activated caffeine mode"));

                let (_path,): (OwnedObjectPath,) =
                    proxy.call("Inhibit", &("", 8u32, options)).await?;

                debug!("[caffeine] Portal inhibit successful");
            }
            Backend::ScreenSaver { path } => {
                let proxy = zbus::Proxy::new(
                    &self.conn,
                    SCREENSAVER_DESTINATION,
                    *path,
                    SCREENSAVER_INTERFACE,
                )
                .await?;

                let (cookie,): (u32,) = proxy
                    .call("Inhibit", &("waft-overview", "User activated caffeine mode"))
                    .await?;

                self.lock_state().screensaver_cookie = Some(cookie);
                debug!("[caffeine] ScreenSaver inhibit cookie: {}", cookie);
            }
        }
        self.lock_state().active = true;
        Ok(())
    }

    async fn uninhibit(&self) -> Result<()> {
        match &self.backend {
            Backend::Portal => {
                // Portal inhibition tied to request lifetime — just update state
                warn!("[caffeine] Portal uninhibit: inhibition releases when daemon restarts");
            }
            Backend::ScreenSaver { path } => {
                let cookie = self.lock_state().screensaver_cookie.take();
                if let Some(cookie) = cookie {
                    let proxy = zbus::Proxy::new(
                        &self.conn,
                        SCREENSAVER_DESTINATION,
                        *path,
                        SCREENSAVER_INTERFACE,
                    )
                    .await?;

                    let _: () = proxy.call("UnInhibit", &(cookie,)).await?;
                    debug!("[caffeine] ScreenSaver uninhibit successful");
                }
            }
        }
        self.lock_state().active = false;
        Ok(())
    }
}

#[async_trait::async_trait]
impl PluginDaemon for CaffeineDaemon {
    fn get_widgets(&self) -> Vec<NamedWidget> {
        let state = self.lock_state();
        vec![NamedWidget {
            id: "caffeine:toggle".to_string(),
            weight: 65,
            widget: FeatureToggleBuilder::new("Caffeine")
                .icon("changes-allow-symbolic")
                .active(state.active)
                .busy(state.busy)
                .on_toggle("toggle")
                .build(),
        }]
    }

    async fn handle_action(
        &self,
        _widget_id: String,
        action: Action,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        if action.id == "toggle" {
            self.lock_state().busy = true;
            let was_active = self.lock_state().active;
            let result = if was_active {
                self.uninhibit().await
            } else {
                self.inhibit().await
            };

            self.lock_state().busy = false;

            if let Err(e) = result {
                log::error!("[caffeine] Toggle failed: {}", e);
                return Err(e.into());
            }
        }
        Ok(())
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    waft_plugin_sdk::init_daemon_logger("info");
    info!("Starting caffeine daemon...");

    let daemon = CaffeineDaemon::new().await?;
    let (server, _notifier) = PluginServer::new("caffeine-daemon", daemon);
    server.run().await?;

    Ok(())
}
