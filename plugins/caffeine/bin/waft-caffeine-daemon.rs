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

use std::sync::OnceLock;

use anyhow::{Context, Result, bail};
use log::{debug, info, warn};
use std::collections::HashMap;
use std::sync::Mutex as StdMutex;
use waft_i18n::I18n;
use waft_plugin::*;
use zbus::Connection;
use zbus::zvariant::{OwnedObjectPath, Value};

static I18N: OnceLock<I18n> = OnceLock::new();

fn i18n() -> &'static I18n {
    I18N.get_or_init(|| {
        I18n::new(&[
            ("en-US", include_str!("../locales/en-US/caffeine.ftl")),
            ("cs-CZ", include_str!("../locales/cs-CZ/caffeine.ftl")),
        ])
    })
}

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
            info!("[caffeine] ScreenSaver backend available at {}", path);
            return Ok(Backend::ScreenSaver { path });
        }
    }

    bail!("No screen inhibit backend available")
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
            .context("Failed to connect to session bus")?;

        let backend = probe_backend(&conn).await?;
        info!("[caffeine] Using backend: {:?}", backend);

        Ok(Self {
            conn,
            backend,
            state: StdMutex::new(CaffeineState {
                active: false,
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
                    .call(
                        "Inhibit",
                        &("waft-overview", "User activated caffeine mode"),
                    )
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
impl Plugin for CaffeinePlugin {
    fn get_entities(&self) -> Vec<Entity> {
        let state = self.lock_state();
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
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        if action == "toggle" {
            let was_active = self.lock_state().active;
            let result = if was_active {
                self.uninhibit().await
            } else {
                self.inhibit().await
            };

            if let Err(e) = result {
                log::error!("[caffeine] Toggle failed: {e}");
                return Err(e.into());
            }
        }
        Ok(())
    }

    fn can_stop(&self) -> bool {
        // Cannot stop gracefully while actively inhibiting
        !self.lock_state().active
    }
}

fn main() -> Result<()> {
    // Handle `provides` CLI command before starting runtime
    if waft_plugin::manifest::handle_provides_i18n(
        &[entity::session::SLEEP_INHIBITOR_ENTITY_TYPE],
        i18n(),
        "plugin-name",
        "plugin-description",
    ) {
        return Ok(());
    }

    // Initialize logging
    waft_plugin::init_plugin_logger("info");

    info!("Starting caffeine plugin...");

    let rt = tokio::runtime::Runtime::new().context("failed to create tokio runtime")?;
    rt.block_on(async {
        let plugin = CaffeinePlugin::new().await?;
        let (runtime, _notifier) = PluginRuntime::new("caffeine", plugin);
        runtime.run().await?;
        Ok(())
    })
}
