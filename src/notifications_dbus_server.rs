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
//! Implemented methods:
//! - `GetCapabilities`
//! - `GetServerInformation`
//! - `Notify`
//! - `CloseNotification`
//! - `GetInhibited` (KDE-compatible, best-effort)
//! - `SetInhibited` (KDE-compatible, best-effort)
//!
//! Implemented signals:
//! - `ActionInvoked`
//! - `NotificationClosed`
//! - `InhibitedChanged` (best-effort; non-standard but useful for syncing UI)
//!
//! This module intentionally keeps the UI layer DBus-agnostic by translating DBus calls into
//! `IngressEvent`s and translating `OutboundEvent`s into DBus signals.

use std::collections::HashMap;
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, AtomicU32, Ordering},
};

use anyhow::{Context, Result, anyhow};
use tokio::sync::mpsc;
use zbus::fdo;
use zbus::names::{BusName, WellKnownName};
use zbus::object_server::SignalEmitter;
use zbus::{Connection, connection::Builder as ConnectionBuilder};
use zvariant::{Array, OwnedValue, Value};

use crate::notifications_dbus::{
    ActionSpec, HintValue, IconSpec, IngressEvent, NotifyRequest, OutboundEvent,
    advertised_capabilities, close_reasons,
};

const BUS_NAME: &str = "org.freedesktop.Notifications";
const OBJECT_PATH: &str = "/org/freedesktop/Notifications";

/// A DBus notifications server that owns `org.freedesktop.Notifications`.
///
/// Intended usage (from `main.rs` startup, before GTK runs):
/// - create `(ingress_tx, ingress_rx)` and `(outbound_tx, outbound_rx)`
/// - `NotificationsDbusServer::connect().await?`
/// - `server.start(ingress_tx, outbound_rx).await?`
/// - pass `ingress_rx` + `outbound_tx` into `NotificationsPlugin::with_dbus_ingress(...)`
///
/// KDE compatibility note:
/// Some daemons (and KDE clients) use a non-standard inhibition flag. We expose it via
/// `GetInhibited` / `SetInhibited` and an in-memory flag that is per-session only.
pub struct NotificationsDbusServer {
    next_id: Arc<AtomicU32>,
    inhibited: Arc<AtomicBool>,
}

impl NotificationsDbusServer {
    /// Construct a server instance. This does not touch DBus yet.
    pub async fn connect() -> Result<Self> {
        Ok(Self {
            next_id: Arc::new(AtomicU32::new(1)),
            inhibited: Arc::new(AtomicBool::new(false)),
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
        &self,
        ingress_tx: mpsc::UnboundedSender<IngressEvent>,
        outbound_rx: mpsc::UnboundedReceiver<OutboundEvent>,
    ) -> Result<()> {
        // Policy: try to replace any existing owner of `org.freedesktop.Notifications` on startup.
        //
        // We do a best-effort call to `ReleaseName` to encourage a cooperative daemon to step down.
        // Even if that fails (most daemons won't release), we still attempt to request the name
        // with replacement flags during connection setup below.
        {
            let probe = Connection::session()
                .await
                .context("Failed to connect to DBus session bus (probe)")?;
            let dbus = fdo::DBusProxy::new(&probe)
                .await
                .context("Failed to create org.freedesktop.DBus proxy (probe)")?;

            let bus_name: BusName<'_> = BusName::try_from(BUS_NAME)
                .map_err(|e| anyhow!(e))
                .context("Invalid BusName for NameHasOwner")?;

            if dbus
                .name_has_owner(bus_name.clone())
                .await
                .context("DBus NameHasOwner(org.freedesktop.Notifications) failed")?
            {
                // Ignore the result: other owners may refuse to release, and that's fine.
                //
                // NOTE: `ReleaseName` expects a well-known name type.
                let well_known: WellKnownName<'_> = WellKnownName::try_from(BUS_NAME)
                    .map_err(|e| anyhow!(e))
                    .context("Invalid WellKnownName for ReleaseName")?;

                let _ = dbus.release_name(well_known).await;
            }
        }

        let service =
            NotificationsService::new(self.next_id.clone(), self.inhibited.clone(), ingress_tx);

        let name = WellKnownName::try_from(BUS_NAME)
            .map_err(|e| anyhow!(e))
            .context("Invalid well-known DBus name for notifications server")?;

        // Build the server connection and export the object.
        //
        // Important: we want to replace any existing owner if possible.
        // `zbus`' builder uses the well-known name request internally; we rely on its default
        // behavior here after attempting a best-effort `ReleaseName` above.
        //
        // If replacement isn't possible (e.g. the bus/owner refuses), this will still fail and
        // the caller will surface the error.
        let conn = ConnectionBuilder::session()
            .context("Failed to create DBus ConnectionBuilder")?
            .name(name)
            .context("Failed to request org.freedesktop.Notifications name")?
            .serve_at(OBJECT_PATH, service.clone())
            .context("Failed to serve notifications object at /org/freedesktop/Notifications")?
            .build()
            .await
            .context("Failed to build DBus server connection")?;

        // Spawn outbound signal loop.
        tokio::spawn(outbound_signal_loop(conn.clone(), outbound_rx));

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
    inhibited: Arc<AtomicBool>,
    ingress_tx: mpsc::UnboundedSender<IngressEvent>,
    _last_sender: Mutex<Option<String>>,
}

impl NotificationsService {
    fn new(
        next_id: Arc<AtomicU32>,
        inhibited: Arc<AtomicBool>,
        ingress_tx: mpsc::UnboundedSender<IngressEvent>,
    ) -> Self {
        Self {
            inner: Arc::new(NotificationsServiceInner {
                next_id,
                inhibited,
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

    async fn emit_inhibited_changed(emitter: &SignalEmitter<'_>, inhibited: bool) {
        let _ = emitter
            .emit(
                "org.freedesktop.Notifications",
                "InhibitedChanged",
                &(inhibited),
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

    /// KDE-compatible inhibition getter (non-standard).
    ///
    /// This is intentionally per-session only (in-memory).
    fn get_inhibited(&self) -> bool {
        self.inner.inhibited.load(Ordering::Relaxed)
    }

    /// KDE-compatible inhibition setter (non-standard).
    ///
    /// This is intentionally per-session only (in-memory).
    async fn set_inhibited(
        &self,
        #[zbus(signal_emitter)] emitter: SignalEmitter<'_>,
        inhibited: bool,
    ) -> fdo::Result<()> {
        self.inner.inhibited.store(inhibited, Ordering::Relaxed);

        // Best-effort signal for any listeners (including our own UI, if it ever chooses to listen).
        NotificationsService::emit_inhibited_changed(&emitter, inhibited).await;

        // Also notify the in-process UI/plugin so it can apply server-side DND gating.
        let _ = self
            .inner
            .ingress_tx
            .send(IngressEvent::InhibitedChanged { inhibited });

        Ok(())
    }

    /// Notify(s app_name, u replaces_id, s app_icon, s summary, s body, as actions, a{sv} hints, i expire_timeout) -> u
    ///
    /// Behavioral notes (per user decisions / AGENTS.md):
    /// - Return DBus-generated IDs.
    /// - `replaces_id`: receiver creates new notification and removes the old one.
    async fn notify(
        &self,
        #[zbus(header)] header: zbus::message::Header<'_>,
        app_name: String,
        replaces_id: u32,
        app_icon: String,
        summary: String,
        body: String,
        actions: Vec<String>,
        hints: HashMap<String, OwnedValue>,
        expire_timeout: i32,
    ) -> fdo::Result<u32> {
        if let Some(sender) = header.sender().map(|s| s.to_string()) {
            let mut guard = self.inner._last_sender.lock().unwrap();
            *guard = Some(sender);
        }

        let id = self.allocate_id();

        // Decode hints once so we can both:
        // - feed them into `NotifyRequest`, and
        // - inspect them for icon-related data.
        let decoded_hints = decode_hints(hints);

        let icon = build_icon_spec(&app_icon, &decoded_hints);

        let request = NotifyRequest {
            app_name,
            replaces_id,
            summary,
            body,
            actions: parse_actions(actions),
            hints: decoded_hints,
            expire_timeout_ms: expire_timeout,
            icon,
        };

        let _ = self
            .inner
            .ingress_tx
            .send(IngressEvent::Notify { id, request });

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
    mut outbound_rx: mpsc::UnboundedReceiver<OutboundEvent>,
) {
    let emitter = match SignalEmitter::new(&conn, OBJECT_PATH) {
        Ok(e) => e,
        Err(_) => return,
    };

    while let Some(ev) = outbound_rx.recv().await {
        match ev {
            OutboundEvent::ActionInvoked { id, action_key } => {
                NotificationsService::emit_action_invoked(&emitter, id, &action_key).await;
            }
            OutboundEvent::NotificationClosed { id, reason } => {
                NotificationsService::emit_notification_closed(&emitter, id, reason).await;
            }
        }
    }
}

fn build_icon_spec(app_icon: &str, hints: &HashMap<String, HintValue>) -> Option<IconSpec> {
    // Priority:
    // 1) `image-path` hint (string, treated as file path)
    // 2) `image-data` hint (bytes)
    // 3) `app_icon` argument (path-like => FilePath, otherwise Themed)
    // 4) None (no explicit icon; UI/plugin will derive app icon / default later)

    if let Some(HintValue::String(path)) = hints.get("image-path") {
        let trimmed = path.trim();
        if !trimmed.is_empty() {
            return Some(IconSpec::FilePath(trimmed.into()));
        }
    }

    if let Some(HintValue::Bytes(bytes)) = hints.get("image-data") {
        if !bytes.is_empty() {
            return Some(IconSpec::Bytes(bytes.clone()));
        }
    }

    let icon = app_icon.trim();
    if !icon.is_empty() {
        // Heuristic: treat as path if it contains a path separator or starts like a path.
        if icon.contains('/') || icon.starts_with('.') || icon.starts_with('~') {
            return Some(IconSpec::FilePath(icon.into()));
        } else {
            return Some(IconSpec::Themed(icon.to_string()));
        }
    }

    None
}

fn parse_actions(actions_raw: Vec<String>) -> Vec<ActionSpec> {
    // Spec: alternating action_key, label.
    let mut out = Vec::new();
    let mut it = actions_raw.into_iter();
    loop {
        let Some(key) = it.next() else { break };
        let Some(label) = it.next() else { break };
        out.push(ActionSpec { key, label });
    }
    out
}

fn decode_hints(hints: HashMap<String, OwnedValue>) -> HashMap<String, HintValue> {
    // Best-effort decoding. Unknown/unsupported variants are ignored.
    let mut out = HashMap::new();
    for (k, v) in hints {
        if let Some(h) = decode_hint_value(&v) {
            // Keep `HintValue` payload fields exercised so we don't accumulate dead-code warnings
            // as hint coverage evolves. This is a no-op apart from a tiny counter increment.
            crate::notifications_dbus::note_hint_value_decoded(&h);
            out.insert(k, h);
        }
    }
    out
}

fn decode_hint_value(v: &OwnedValue) -> Option<HintValue> {
    // Conservative subset of types commonly used in notification hints:
    // - bool, i32/u32, i64/u64, f64, string, bytes (ay).
    //
    // In zvariant v5, `OwnedValue::downcast_ref::<T>()` returns `Result<T, _>`.
    let val: Value<'_> = match v.downcast_ref::<Value>() {
        Ok(r) => r,
        Err(_) => return None,
    };

    match val {
        Value::Bool(b) => Some(HintValue::Bool(b)),
        Value::I16(i) => Some(HintValue::I32(i as i32)),
        Value::I32(i) => Some(HintValue::I32(i)),
        Value::I64(i) => Some(HintValue::I64(i)),
        Value::U16(u) => Some(HintValue::U32(u as u32)),
        Value::U32(u) => Some(HintValue::U32(u)),
        Value::U64(u) => Some(HintValue::U64(u)),
        Value::F64(f) => Some(HintValue::F64(f)),
        Value::Str(s) => Some(HintValue::String(s.to_string())),
        Value::Signature(s) => Some(HintValue::String(s.to_string())),
        Value::Array(a) => decode_bytes_array(a),
        _ => None,
    }
}

fn decode_bytes_array(a: Array<'_>) -> Option<HintValue> {
    // Only accept `ay` (array of u8).
    let mut bytes: Vec<u8> = Vec::new();
    for item in a.iter() {
        match item {
            Value::U8(b) => bytes.push(*b),
            _ => return None,
        }
    }
    if bytes.is_empty() {
        None
    } else {
        Some(HintValue::Bytes(bytes))
    }
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
