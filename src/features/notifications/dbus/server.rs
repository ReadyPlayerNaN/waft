//! DBus notifications server (`org.freedesktop.Notifications`) implemented with `zbus`.
//!
//! This module is the *server side* implementation of the freedesktop.org notifications spec.
//! The app owns the well-known name `org.freedesktop.Notifications` on the session bus and
//! exports `/org/freedesktop/Notifications` implementing `org.freedesktop.Notifications`.
//!
//! Design constraints (see `AGENTS.md`):
//! - GTK must remain on the main thread.
//! - This server runs on tokio and communicates with the notifications plugin/controller
//!   via channels using `crate::notifications_dbus::{IngressEvent, OutboundEvent}`.
//! - If `org.freedesktop.Notifications` is already owned, startup should attempt to replace the owner.
//!
//! This module intentionally keeps the UI layer DBus-agnostic by translating DBus calls into
//! `IngressEvent`s and translating `OutboundEvent`s into DBus signals.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex};
use std::time::SystemTime;

use anyhow::{Context, Result, anyhow};
use flume::{Receiver, Sender};
use log::info;
use log::warn;
use zbus::fdo;
use zbus::names::WellKnownName;
use zbus::object_server::SignalEmitter;
use zbus::{Connection, connection::Builder as ConnectionBuilder};
use zvariant::OwnedValue;

use super::client::{IngressEvent, OutboundEvent, advertised_capabilities, close_reasons};
use super::hints::{decode_hints, parse_hints};
use super::ingress::IngressedNotification;

const BUS_NAME: &str = "org.freedesktop.Notifications";
const OBJECT_PATH: &str = "/org/freedesktop/Notifications";

/// A DBus notifications server that owns `org.freedesktop.Notifications`.
///
/// Intended usage (from `main.rs` startup, before GTK runs):
/// - create `(ingress_tx, ingress_rx)` and `(outbound_tx, outbound_rx)`
/// - `NotificationsDbusServer::connect().await?`
/// - `server.start(ingress_tx, outbound_rx).await?`
/// - pass `ingress_rx` + `outbound_tx` into `NotificationsPlugin::with_dbus_ingress(...)`
pub struct NotificationsDbusServer {
    next_id: Arc<AtomicU32>,
    connection: Option<Connection>,
}

impl NotificationsDbusServer {
    /// Construct a server instance. This does not touch DBus yet.
    pub async fn connect() -> Result<Self> {
        Ok(Self {
            next_id: Arc::new(AtomicU32::new(1)),
            connection: None,
        })
    }

    /// Start the DBus server.
    ///
    /// This will:
    /// - connect to the session bus
    /// - request (and attempt to replace) `org.freedesktop.Notifications` if it is already owned
    /// - export `/org/freedesktop/Notifications`
    /// - spawn a background task translating `OutboundEvent` -> DBus signals
    pub async fn start(
        &mut self,
        ingress_tx: Sender<IngressEvent>,
        outbound_rx: Receiver<OutboundEvent>,
    ) -> Result<()> {
        info!("Starting notifications service");

        let service = NotificationsService::new(self.next_id.clone(), ingress_tx);

        let name = WellKnownName::try_from(BUS_NAME)
            .map_err(|e| anyhow!(e))
            .context("Invalid well-known DBus name for notifications server")?;

        info!("Starting notifications dbus connection");

        // Build the server connection without requesting the name yet
        let conn = ConnectionBuilder::session()
            .context("Failed to create DBus ConnectionBuilder")?
            .serve_at(OBJECT_PATH, service.clone())
            .context("Failed to serve notifications object at /org/freedesktop/Notifications")?
            .build()
            .await
            .context("Failed to build DBus server connection")?;

        // Now manually request the name with replacement flags
        let dbus = fdo::DBusProxy::new(&conn)
            .await
            .context("Failed to create org.freedesktop.DBus proxy")?;

        // Use REPLACE_EXISTING and ALLOW_REPLACEMENT flags to take over from existing owners
        let flags =
            fdo::RequestNameFlags::ReplaceExisting | fdo::RequestNameFlags::AllowReplacement;

        let result = dbus
            .request_name(name.clone(), flags)
            .await
            .context("Failed to request org.freedesktop.Notifications name")?;

        match result {
            fdo::RequestNameReply::PrimaryOwner => {
                info!("Successfully acquired org.freedesktop.Notifications name as primary owner");
            }
            fdo::RequestNameReply::AlreadyOwner => {
                info!("Already own org.freedesktop.Notifications name");
            }
            fdo::RequestNameReply::InQueue => {
                return Err(anyhow!(
                    "org.freedesktop.Notifications name request queued - replacement failed"
                ));
            }
            fdo::RequestNameReply::Exists => {
                return Err(anyhow!(
                    "org.freedesktop.Notifications name exists and replacement was denied"
                ));
            }
        }

        info!("Successfully started notifications DBus server");

        // Spawn outbound signal loop.
        relm4::tokio::spawn(outbound_signal_loop(conn.clone(), outbound_rx));

        // Store the connection to keep it alive
        self.connection = Some(conn);

        Ok(())
    }
}

/// Internal DBus interface implementation.
///
/// This type is registered at `/org/freedesktop/Notifications`.
#[derive(Clone)]
struct NotificationsService {
    inner: Arc<NotificationsServiceInner>,
}

struct NotificationsServiceInner {
    next_id: Arc<AtomicU32>,
    ingress_tx: Sender<IngressEvent>,
    _last_sender: Mutex<Option<String>>,
}

impl NotificationsService {
    fn new(next_id: Arc<AtomicU32>, ingress_tx: Sender<IngressEvent>) -> Self {
        Self {
            inner: Arc::new(NotificationsServiceInner {
                next_id,
                ingress_tx,
                _last_sender: Mutex::new(None),
            }),
        }
    }

    fn allocate_id(&self) -> u32 {
        // Ensure we never return 0.
        let id = self.inner.next_id.fetch_add(1, Ordering::Relaxed);
        if id == 0 {
            // Wrapped to 0; skip to 1.
            self.inner.next_id.store(2, Ordering::Relaxed);
            1
        } else {
            id
        }
    }

    async fn emit_action_invoked(emitter: &SignalEmitter<'_>, id: u32, action_key: &str) {
        // zbus v5: use `emit` on `SignalEmitter` and provide (interface, member, args).
        let _ = emitter
            .emit(
                "org.freedesktop.Notifications",
                "ActionInvoked",
                &(id, action_key),
            )
            .await;
    }

    async fn emit_notification_closed(emitter: &SignalEmitter<'_>, id: u32, reason: u32) {
        let _ = emitter
            .emit(
                "org.freedesktop.Notifications",
                "NotificationClosed",
                &(id, reason),
            )
            .await;
    }
}

#[zbus::interface(name = "org.freedesktop.Notifications")]
impl NotificationsService {
    /// GetCapabilities() -> as
    fn get_capabilities(&self) -> Vec<String> {
        advertised_capabilities()
            .into_iter()
            .map(|s| s.to_string())
            .collect()
    }

    /// GetServerInformation() -> (s, s, s, s)
    ///
    /// Returns: (name, vendor, version, spec_version)
    fn get_server_information(&self) -> (String, String, String, String) {
        (
            "sacrebleui".to_string(),
            "sacrebleui".to_string(),
            env!("CARGO_PKG_VERSION").to_string(),
            "1.2".to_string(),
        )
    }

    /// Notify(s app_name, u replaces_id, s app_icon, s summary, s body, as actions, a{sv} hints, i expire_timeout) -> u
    ///
    /// Behavioral notes (per user decisions / AGENTS.md):
    /// - Return DBus-generated IDs.
    /// - `replaces_id`: receiver creates new notification and removes the old one.
    async fn notify(
        &self,
        #[zbus(header)] header: zbus::message::Header<'_>,
        app_name: &str,
        replaces_id: u32,
        icon: &str,
        summary: &str,
        body: &str,
        actions: Vec<String>,
        hints: HashMap<String, OwnedValue>,
        expire_timeout: i32,
    ) -> fdo::Result<u32> {
        if let Some(sender) = header.sender().map(|s| s.to_string()) {
            let mut guard = self.inner._last_sender.lock().unwrap();
            *guard = Some(sender);
        }

        let id = self.allocate_id();
        let hints =
            parse_hints(&decode_hints(hints)).map_err(|e| fdo::Error::Failed(e.to_string()))?;

        let notification = IngressedNotification {
            actions: actions.into_iter().map(|s| Arc::from(s)).collect(),
            app_name: match app_name.is_empty() {
                false => Some(Arc::from(app_name)),
                _ => None,
            },
            created_at: SystemTime::now(),
            description: Arc::from(body),
            hints: hints,
            icon: match icon.is_empty() {
                false => Some(Arc::from(icon)),
                _ => None,
            },
            id: id as u64,
            replaces_id: match replaces_id {
                0 => None,
                _ => Some(replaces_id as u64),
            },
            title: Arc::from(summary),
            ttl: if expire_timeout == 0 {
                None
            } else if expire_timeout < 0 {
                Some(0)
            } else {
                Some(expire_timeout as u64)
            },
        };

        let _ = self.inner.ingress_tx.send(IngressEvent::Notify {
            notification: notification,
        });

        Ok(id)
    }

    /// CloseNotification(u id) -> ()
    ///
    /// The UI/plugin will remove and then emit NotificationClosed(reason=CLOSED_BY_CALL).
    async fn close_notification(&self, id: u32) -> fdo::Result<()> {
        let _ = self
            .inner
            .ingress_tx
            .send(IngressEvent::CloseNotification { id });
        Ok(())
    }
}

async fn outbound_signal_loop(
    conn: Connection,
    outbound_rx: Receiver<OutboundEvent>,
) -> Result<()> {
    let emitter = match SignalEmitter::new(&conn, OBJECT_PATH) {
        Ok(e) => e,
        Err(_) => {
            warn!("Failed to create signal emitter");
            return Ok(());
        }
    };

    loop {
        match outbound_rx.recv_async().await {
            Ok(ev) => {
                match ev {
                    OutboundEvent::ActionInvoked { id, action_key } => {
                        NotificationsService::emit_action_invoked(&emitter, id, &action_key).await;
                    }
                    OutboundEvent::NotificationClosed { id, reason } => {
                        NotificationsService::emit_notification_closed(&emitter, id, reason).await;
                    }
                };
            }
            Err(_) => break,
        }
    }
    Ok(())
}

// Keep the close reason constants referenced somewhere so the module stays aligned with policy,
// even if the server doesn't emit close signals for actions directly.
#[allow(unused)]
fn _close_reason_policy_guards() -> (u32, u32, u32, u32) {
    (
        close_reasons::EXPIRED,
        close_reasons::DISMISSED_BY_USER,
        close_reasons::CLOSED_BY_CALL,
        close_reasons::UNDEFINED,
    )
}
