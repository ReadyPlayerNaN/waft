//! Caffeine daemon — screen lock/screensaver inhibition toggle.
//!
//! Provides a `sleep-inhibitor` entity that can be toggled to prevent screen
//! locking via xdg-desktop-portal (Inhibit) or org.freedesktop.ScreenSaver
//! as fallback.
//!
//! Configuration (in ~/.config/waft/config.toml):
//! ```toml
//! [[plugins]]
//! id = "caffeine"
//! ```

use std::sync::LazyLock;

use anyhow::Result;
use log::{debug, info, warn};
use std::collections::HashMap;
use std::sync::Mutex as StdMutex;
use waft_plugin::*;
use zbus::Connection;
use zbus::zvariant::{OwnedObjectPath, Value};

static I18N: LazyLock<waft_i18n::I18n> = LazyLock::new(|| waft_i18n::I18n::new(&[
    ("en-US", include_str!("../locales/en-US/caffeine.ftl")),
    ("cs-CZ", include_str!("../locales/cs-CZ/caffeine.ftl")),
]));

fn i18n() -> &'static waft_i18n::I18n { &I18N }

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
        let proxy =
            zbus::Proxy::new(conn, SCREENSAVER_DESTINATION, *path, SCREENSAVER_INTERFACE).await?;
        if proxy.call::<_, _, (bool,)>("GetActive", &()).await.is_ok() {
            info!("[caffeine] ScreenSaver backend available at {path}");
            return Ok(Backend::ScreenSaver { path });
        }
    }

    anyhow::bail!("No screen inhibit backend available")
}

/// Mutable caffeine state behind interior mutability.
struct CaffeineState {
    active: bool,
    screensaver_cookie: Option<u32>,
}

struct CaffeinePlugin {
    conn: Connection,
    backend: Backend,
    state: StdMutex<CaffeineState>,
}

impl CaffeinePlugin {
    async fn new() -> Result<Self> {
        let conn = Connection::session()
            .await
            .map_err(|e| anyhow::anyhow!("Failed to connect to session bus: {e}"))?;

        let backend = probe_backend(&conn).await?;
        info!("[caffeine] Using backend: {backend:?}");

        Ok(Self {
            conn,
            backend,
            state: StdMutex::new(CaffeineState {
                active: false,
                screensaver_cookie: None,
            }),
        })
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
                    .call(
                        "Inhibit",
                        &("waft-overview", "User activated caffeine mode"),
                    )
                    .await?;

                self.state.lock_or_recover().screensaver_cookie = Some(cookie);
                debug!("[caffeine] ScreenSaver inhibit cookie: {cookie}");
            }
        }
        self.state.lock_or_recover().active = true;
        Ok(())
    }

    async fn uninhibit(&self) -> Result<()> {
        match &self.backend {
            Backend::Portal => {
                // Portal inhibition tied to request lifetime — just update state
                warn!("[caffeine] Portal uninhibit: inhibition releases when daemon restarts");
            }
            Backend::ScreenSaver { path } => {
                let cookie = self.state.lock_or_recover().screensaver_cookie.take();
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
        self.state.lock_or_recover().active = false;
        Ok(())
    }
}

#[async_trait::async_trait]
impl Plugin for CaffeinePlugin {
    fn get_entities(&self) -> Vec<Entity> {
        let state = self.state.lock_or_recover();
        let inhibitor = entity::session::SleepInhibitor {
            active: state.active,
        };
        vec![Entity::new(
            Urn::new(
                "caffeine",
                entity::session::SLEEP_INHIBITOR_ENTITY_TYPE,
                "default",
            ),
            entity::session::SLEEP_INHIBITOR_ENTITY_TYPE,
            &inhibitor,
        )]
    }

    async fn handle_action(
        &self,
        _urn: Urn,
        action: String,
        _params: serde_json::Value,
    ) -> anyhow::Result<serde_json::Value> {
        if action == "toggle" {
            let was_active = self.state.lock_or_recover().active;
            let result = if was_active {
                self.uninhibit().await
            } else {
                self.inhibit().await
            };

            if let Err(e) = result {
                log::error!("[caffeine] Toggle failed: {e}");
                return Err(e);
            }
        }
        Ok(serde_json::Value::Null)
    }

    fn can_stop(&self) -> bool {
        // Cannot stop gracefully while actively inhibiting
        !self.state.lock_or_recover().active
    }
}

fn main() -> Result<()> {
    PluginRunner::new("caffeine", &[entity::session::SLEEP_INHIBITOR_ENTITY_TYPE])
        .i18n(i18n(), "plugin-name", "plugin-description")
        .run(|_notifier| async {
            CaffeinePlugin::new().await
        })
}
